//! Clipboard history: capture, de-duplication, retention, search and privacy controls.
//!
//! Policies (also documented in docs/PRIVACY.md):
//! * Text only. Empty / whitespace-only copies are ignored.
//! * De-duplication: identical content (exact match, SHA-256) is never stored twice; copying
//!   it again moves the existing entry to the top and increments its copy count.
//! * Retention runs at startup, after every capture and whenever settings change, in this
//!   order: (1) delete unpinned items unused for longer than the retention period,
//!   (2) keep only the newest `max_items` unpinned items, (3) keep unpinned items within the
//!   aggregate size budget, deleting oldest first. Pinned items are never removed by cleanup.
//! * Best-effort secret detection skips content that looks like a password, key or token.
//!   It is a heuristic and is not guaranteed to catch everything.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::settings::Settings;
use crate::util::{now_ms, sha256_hex, truncate_chars};
use regex::Regex;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::sync::LazyLock;

/// Upper bound on the total size of unpinned history, regardless of item count.
pub const AGGREGATE_LIMIT_BYTES: i64 = 25 * 1024 * 1024;
pub const MAX_PINNED: i64 = 100;
const PREVIEW_CHARS: usize = 400;

#[derive(Debug, PartialEq)]
pub enum CaptureOutcome {
    Inserted(i64),
    Bumped(i64),
    Ignored,
    TooLarge(usize),
    LikelySecret,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: i64,
    pub preview: String,
    pub truncated: bool,
    pub content_type: String,
    pub created_at: i64,
    pub last_used_at: i64,
    pub pinned: bool,
    pub size_bytes: i64,
    pub copy_count: i64,
    pub line_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipPage {
    pub items: Vec<ClipItem>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipStats {
    pub count: i64,
    pub pinned: i64,
    pub total_bytes: i64,
}

static SECRET_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
        r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b", // JWT
        r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}\b",                    // GitHub tokens
        r"\bgithub_pat_[A-Za-z0-9_]{40,}\b",
        r"\bglpat-[A-Za-z0-9_-]{20,}\b",                                    // GitLab
        r"\bxox[abposr]-[A-Za-z0-9-]{10,}\b",                               // Slack
        r"\bsk-(?:proj-|ant-)?[A-Za-z0-9_-]{20,}\b",                        // OpenAI / Anthropic style
        r"\bAKIA[0-9A-Z]{16}\b",                                            // AWS access key id
        r"\bAIza[0-9A-Za-z_-]{35}\b",                                       // Google API key
        r"\b(?:sk|rk)_live_[A-Za-z0-9]{20,}\b",                             // Stripe
    ]
    .iter()
    .map(|p| Regex::new(p).expect("valid secret regex"))
    .collect()
});

/// `password = hunter22` style lines (including .env files). The value must look like a
/// literal (no spaces, quotes or call syntax) and contain a digit, so code such as
/// `password: string;` or `token = getToken()` is not mistaken for a secret.
static KEY_VALUE_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?im)^\s*["']?(?:password|passwd|pwd|secret|client[_-]?secret|api[_-]?key|access[_-]?token|auth[_-]?token)["']?\s*[:=]\s*["']?([^\s"'(),;]{6,})["']?\s*[,;]?\s*$"#,
    )
    .expect("valid key/value regex")
});

fn shannon_entropy(s: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    let mut total = 0f64;
    for c in s.chars() {
        *counts.entry(c).or_insert(0f64) += 1.0;
        total += 1.0;
    }
    counts.values().map(|&n| {
        let p = n / total;
        -p * p.log2()
    }).sum()
}

/// Heuristic: does this text look like a password, API key or token?
pub fn is_likely_secret(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if SECRET_PATTERNS.iter().any(|re| re.is_match(t)) {
        return true;
    }
    if KEY_VALUE_SECRET
        .captures_iter(t)
        .any(|c| c.get(1).is_some_and(|v| v.as_str().chars().any(|ch| ch.is_ascii_digit())))
    {
        return true;
    }
    // A single "word" of 12–128 chars mixing at least three character classes with high
    // entropy looks like a generated password. URLs, paths and e-mail addresses are excluded.
    let len = t.chars().count();
    if !(12..=128).contains(&len) || t.chars().any(char::is_whitespace) {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("www.") || t.contains('\\') || t.starts_with('/') || t.contains('@') {
        return false;
    }
    let classes = [
        t.chars().any(|c| c.is_ascii_lowercase()),
        t.chars().any(|c| c.is_ascii_uppercase()),
        t.chars().any(|c| c.is_ascii_digit()),
        t.chars().any(|c| !c.is_ascii_alphanumeric()),
    ]
    .iter()
    .filter(|&&b| b)
    .count();
    classes >= 3 && shannon_entropy(t) >= 3.3
}

/// Stores captured text according to the current settings.
pub fn capture(db: &Db, settings: &Settings, text: &str, now: i64) -> AppResult<CaptureOutcome> {
    if text.trim().is_empty() {
        return Ok(CaptureOutcome::Ignored);
    }
    let size = text.len();
    if size > settings.clipboard_max_entry_kb as usize * 1024 {
        return Ok(CaptureOutcome::TooLarge(size));
    }
    if settings.clipboard_skip_secrets && is_likely_secret(text) {
        return Ok(CaptureOutcome::LikelySecret);
    }
    let hash = sha256_hex(text.as_bytes());
    let outcome = db.tx(|tx| {
        let existing: Option<i64> =
            tx.query_row("SELECT id FROM clipboard_items WHERE content_hash = ?1", [&hash], |r| r.get(0)).optional()?;
        if let Some(id) = existing {
            tx.execute(
                "UPDATE clipboard_items SET last_used_at = ?2, copy_count = copy_count + 1 WHERE id = ?1",
                params![id, now],
            )?;
            return Ok(CaptureOutcome::Bumped(id));
        }
        tx.execute(
            "INSERT INTO clipboard_items (content, content_type, content_hash, created_at, last_used_at, pinned, size_bytes)
             VALUES (?1, 'text', ?2, ?3, ?3, 0, ?4)",
            params![text, hash, now, size as i64],
        )?;
        Ok(CaptureOutcome::Inserted(tx.last_insert_rowid()))
    })?;
    apply_retention(db, settings, now)?;
    Ok(outcome)
}

/// Deterministic retention cleanup. Returns the number of items removed.
pub fn apply_retention(db: &Db, settings: &Settings, now: i64) -> AppResult<usize> {
    db.tx(|tx| {
        let mut removed = 0usize;
        if settings.clipboard_retention_days > 0 {
            let cutoff = now - settings.clipboard_retention_days as i64 * 86_400_000;
            removed += tx.execute("DELETE FROM clipboard_items WHERE pinned = 0 AND last_used_at < ?1", [cutoff])?;
        }
        removed += tx.execute(
            "DELETE FROM clipboard_items WHERE id IN (
                SELECT id FROM clipboard_items WHERE pinned = 0
                ORDER BY last_used_at DESC, id DESC LIMIT -1 OFFSET ?1)",
            [settings.clipboard_max_items as i64],
        )?;
        removed += tx.execute(
            "DELETE FROM clipboard_items WHERE id IN (
                SELECT id FROM (
                    SELECT id, SUM(size_bytes) OVER (ORDER BY last_used_at DESC, id DESC) AS running
                    FROM clipboard_items WHERE pinned = 0)
                WHERE running > ?1)",
            [AGGREGATE_LIMIT_BYTES],
        )?;
        Ok(removed)
    })
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ClipFilter {
    All,
    Pinned,
}

fn to_item(id: i64, content: &str, content_type: String, created_at: i64, last_used_at: i64, pinned: bool, size: i64, copies: i64) -> ClipItem {
    let preview = truncate_chars(content, PREVIEW_CHARS);
    ClipItem {
        id,
        preview: preview.to_string(),
        truncated: preview.len() < content.len(),
        content_type,
        created_at,
        last_used_at,
        pinned,
        size_bytes: size,
        copy_count: copies,
        line_count: content.lines().count().max(1) as i64,
    }
}

/// Lists entries newest first. Search is a case-insensitive substring match done locally.
pub fn list(db: &Db, query: &str, filter: ClipFilter, limit: usize, offset: usize) -> AppResult<ClipPage> {
    let needle = query.trim().to_lowercase();
    let limit = limit.clamp(1, 500);
    db.with(|c| {
        let sql = match filter {
            ClipFilter::All => "SELECT id, content, content_type, created_at, last_used_at, pinned, size_bytes, copy_count
                                FROM clipboard_items ORDER BY last_used_at DESC, id DESC",
            ClipFilter::Pinned => "SELECT id, content, content_type, created_at, last_used_at, pinned, size_bytes, copy_count
                                   FROM clipboard_items WHERE pinned = 1 ORDER BY last_used_at DESC, id DESC",
        };
        let mut stmt = c.prepare(sql)?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        let mut total = 0i64;
        while let Some(r) = rows.next()? {
            let content: String = r.get(1)?;
            if !needle.is_empty() && !content.to_lowercase().contains(&needle) {
                continue;
            }
            total += 1;
            if (total as usize) > offset && items.len() < limit {
                items.push(to_item(
                    r.get(0)?,
                    &content,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get::<_, i64>(5)? != 0,
                    r.get(6)?,
                    r.get(7)?,
                ));
            }
        }
        Ok(ClipPage { items, total })
    })
}

pub fn content(db: &Db, id: i64) -> AppResult<String> {
    db.with(|c| {
        c.query_row("SELECT content FROM clipboard_items WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| AppError::not_found("That clipboard item"))
    })
}

pub fn mark_used(db: &Db, id: i64) -> AppResult<()> {
    db.with(|c| {
        c.execute("UPDATE clipboard_items SET last_used_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    })
}

pub fn set_pinned(db: &Db, id: i64, pinned: bool) -> AppResult<()> {
    db.tx(|tx| {
        if pinned {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM clipboard_items WHERE pinned = 1 AND id != ?1", [id], |r| r.get(0))?;
            if count >= MAX_PINNED {
                return Err(AppError::invalid(format!("You can pin up to {MAX_PINNED} items. Unpin something first.")));
            }
        }
        let n = tx.execute("UPDATE clipboard_items SET pinned = ?2 WHERE id = ?1", params![id, pinned as i64])?;
        if n == 0 {
            return Err(AppError::not_found("That clipboard item"));
        }
        Ok(())
    })
}

pub fn delete(db: &Db, id: i64) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM clipboard_items WHERE id = ?1", [id])?;
        Ok(())
    })
}

/// Clears history. Pinned items are kept unless `include_pinned` is true.
pub fn clear(db: &Db, include_pinned: bool) -> AppResult<usize> {
    db.with(|c| {
        let n = if include_pinned {
            c.execute("DELETE FROM clipboard_items", [])?
        } else {
            c.execute("DELETE FROM clipboard_items WHERE pinned = 0", [])?
        };
        // Reclaim the space so deleted text does not linger in free pages.
        let _ = c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;");
        Ok(n)
    })
}

pub fn stats(db: &Db) -> AppResult<ClipStats> {
    db.with(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*), COALESCE(SUM(pinned), 0), COALESCE(SUM(size_bytes), 0) FROM clipboard_items",
            [],
            |r| Ok(ClipStats { count: r.get(0)?, pinned: r.get(1)?, total_bytes: r.get(2)? }),
        )?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings { clipboard_max_items: 10, clipboard_retention_days: 7, ..Settings::default() }
    }

    #[test]
    fn dedup_bumps_instead_of_duplicating() {
        let db = Db::open_in_memory().unwrap();
        let s = settings();
        let a = capture(&db, &s, "hello world", 1_000).unwrap();
        let CaptureOutcome::Inserted(id) = a else { panic!("expected insert") };
        assert_eq!(capture(&db, &s, "hello world", 2_000).unwrap(), CaptureOutcome::Bumped(id));
        let page = list(&db, "", ClipFilter::All, 50, 0).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].copy_count, 2);
        assert_eq!(page.items[0].last_used_at, 2_000);
    }

    #[test]
    fn ignores_blank_and_oversized() {
        let db = Db::open_in_memory().unwrap();
        let s = Settings { clipboard_max_entry_kb: 1, ..settings() };
        assert_eq!(capture(&db, &s, "   \n\t", 1).unwrap(), CaptureOutcome::Ignored);
        let big = "x".repeat(1025);
        assert_eq!(capture(&db, &s, &big, 1).unwrap(), CaptureOutcome::TooLarge(1025));
        assert_eq!(stats(&db).unwrap().count, 0);
    }

    #[test]
    fn retention_by_age_count_and_pins() {
        let db = Db::open_in_memory().unwrap();
        let s = settings();
        let day = 86_400_000;
        let now = 100 * day;
        // Old item, pinned old item.
        capture(&db, &s, "old", now - 8 * day).unwrap();
        let CaptureOutcome::Inserted(pinned) = capture(&db, &s, "old pinned", now - 9 * day).unwrap() else { panic!() };
        set_pinned(&db, pinned, true).unwrap();
        for i in 0..15 {
            capture(&db, &s, &format!("item {i}"), now + i).unwrap();
        }
        apply_retention(&db, &s, now + 100).unwrap();
        let page = list(&db, "", ClipFilter::All, 100, 0).unwrap();
        // 10 newest unpinned + 1 pinned. Old unpinned item removed by age.
        assert_eq!(page.total, 11);
        assert!(page.items.iter().any(|i| i.id == pinned && i.pinned));
        assert!(!page.items.iter().any(|i| i.preview == "old"));
        assert!(!page.items.iter().any(|i| i.preview == "item 0"));
        assert!(page.items.iter().any(|i| i.preview == "item 14"));
        // Newest first.
        assert_eq!(page.items[0].preview, "item 14");
    }

    #[test]
    fn retention_zero_days_means_no_age_limit() {
        let db = Db::open_in_memory().unwrap();
        let s = Settings { clipboard_retention_days: 0, ..settings() };
        capture(&db, &s, "ancient", 1).unwrap();
        apply_retention(&db, &s, 1_000 * 86_400_000).unwrap();
        assert_eq!(stats(&db).unwrap().count, 1);
    }

    #[test]
    fn aggregate_size_limit_removes_oldest_unpinned() {
        let db = Db::open_in_memory().unwrap();
        let s = Settings { clipboard_max_items: 5000, clipboard_max_entry_kb: 2048, ..settings() };
        let chunk = 2 * 1024 * 1024 - 16; // just under the 2 MB per-entry cap
        for i in 0..14 {
            let text = format!("{i:02}{}", "a".repeat(chunk));
            assert!(matches!(capture(&db, &s, &text, 1_000 + i).unwrap(), CaptureOutcome::Inserted(_)));
        }
        let st = stats(&db).unwrap();
        assert!(st.total_bytes <= AGGREGATE_LIMIT_BYTES, "total {} over limit", st.total_bytes);
        let page = list(&db, "", ClipFilter::All, 100, 0).unwrap();
        assert!(page.items[0].preview.starts_with("13"));
    }

    #[test]
    fn search_filter_and_paging() {
        let db = Db::open_in_memory().unwrap();
        let s = settings();
        capture(&db, &s, "Alpha Beta", 1).unwrap();
        capture(&db, &s, "gamma", 2).unwrap();
        capture(&db, &s, "ALPHA again", 3).unwrap();
        let page = list(&db, "alpha", ClipFilter::All, 1, 0).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].preview, "ALPHA again");
        let page2 = list(&db, "alpha", ClipFilter::All, 1, 1).unwrap();
        assert_eq!(page2.items[0].preview, "Alpha Beta");
        assert_eq!(list(&db, "", ClipFilter::Pinned, 10, 0).unwrap().total, 0);
    }

    #[test]
    fn pin_limit_and_clear() {
        let db = Db::open_in_memory().unwrap();
        let s = Settings { clipboard_max_items: 1000, ..settings() };
        let mut ids = vec![];
        for i in 0..(MAX_PINNED + 1) {
            if let CaptureOutcome::Inserted(id) = capture(&db, &s, &format!("p{i}"), 10 + i).unwrap() {
                ids.push(id);
            }
        }
        for id in ids.iter().take(MAX_PINNED as usize) {
            set_pinned(&db, *id, true).unwrap();
        }
        assert!(set_pinned(&db, *ids.last().unwrap(), true).is_err());
        assert_eq!(clear(&db, false).unwrap(), 1);
        assert_eq!(stats(&db).unwrap().count, MAX_PINNED);
        clear(&db, true).unwrap();
        assert_eq!(stats(&db).unwrap().count, 0);
    }

    #[test]
    fn secret_heuristics() {
        assert!(is_likely_secret("ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"));
        assert!(is_likely_secret("AKIAIOSFODNN7EXAMPLE"));
        assert!(is_likely_secret("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"));
        assert!(is_likely_secret("-----BEGIN RSA PRIVATE KEY-----\nabc"));
        assert!(is_likely_secret("password: hunter22!"));
        assert!(is_likely_secret("Tr0ub4dor&3xK!q9"));
        assert!(is_likely_secret("sk-proj-abcdefghijklmnopqrstuvwxyz123456"));
        // Ordinary content is kept.
        assert!(!is_likely_secret("Hello, how are you today?"));
        assert!(!is_likely_secret("https://example.com/Path?Query=1&x=Y2"));
        assert!(!is_likely_secret("C:\\Users\\Me\\Documents\\File-01.txt"));
        assert!(!is_likely_secret("someone@example.com"));
        assert!(!is_likely_secret("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0")); // commit SHA
        assert!(!is_likely_secret("npm install --save-dev vitest"));
        assert!(!is_likely_secret("ThisIsCamelCase"));
        assert!(is_likely_secret("DB_HOST=localhost\nAPI_KEY=abc123def456\n"));
        assert!(!is_likely_secret("interface User {\n  password: string;\n}"));
        assert!(!is_likely_secret("const token = getToken();"));
    }

    #[test]
    fn secrets_skipped_only_when_enabled() {
        let db = Db::open_in_memory().unwrap();
        let on = settings();
        assert_eq!(capture(&db, &on, "AKIAIOSFODNN7EXAMPLE", 1).unwrap(), CaptureOutcome::LikelySecret);
        let off = Settings { clipboard_skip_secrets: false, ..settings() };
        assert!(matches!(capture(&db, &off, "AKIAIOSFODNN7EXAMPLE", 1).unwrap(), CaptureOutcome::Inserted(_)));
    }
}
