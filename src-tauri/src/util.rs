use crate::error::{AppError, AppResult};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

/// Current wall-clock time as Unix milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Truncates to at most `max` characters (not bytes), never splitting a code point.
pub fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Runs a command body, converting a panic into a structured error so a bug in one
/// utility can never take down the dock shell (FR-DCK-014).
pub fn guard<T>(name: &str, f: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err(AppError::internal(format!("panic in {name}: {msg}")))
        }
    }
}

/// Validates and normalises a user-entered title-like string.
pub fn clean_text(value: &str, field: &str, min: usize, max: usize) -> AppResult<String> {
    let trimmed = value.trim();
    let len = trimmed.chars().count();
    if len < min {
        return Err(AppError::invalid(format!("{field} is required.")));
    }
    if len > max {
        return Err(AppError::invalid(format!("{field} must be {max} characters or fewer.")));
    }
    Ok(trimmed.to_string())
}

pub fn clean_optional(value: Option<&str>, field: &str, max: usize) -> AppResult<Option<String>> {
    match value.map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) => clean_text(v, field, 1, max).map(Some),
    }
}

/// Spawns a named background thread whose loop is restarted (with back-off) if it panics.
/// Used for long-lived integrations so one failing module cannot stop the others.
pub fn spawn_supervised<F>(name: &'static str, body: F)
where
    F: Fn() -> bool + Send + Sync + 'static,
{
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let mut backoff_ms = 500u64;
            loop {
                match catch_unwind(AssertUnwindSafe(&body)) {
                    // body returned normally: true = asked to stop.
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(_) => log::error!("{name} thread panicked; restarting in {backoff_ms} ms"),
                }
                std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        })
        .expect("failed to spawn thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate_chars("héllo", 2), "hé");
        assert_eq!(truncate_chars("ab", 5), "ab");
        assert_eq!(truncate_chars("😀😀😀", 1), "😀");
    }

    #[test]
    fn sha_is_hex() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn guard_converts_panics() {
        let r: AppResult<()> = guard("t", || panic!("boom"));
        assert_eq!(r.unwrap_err().code, "internal");
    }

    #[test]
    fn clean_text_validates() {
        assert!(clean_text("   ", "Title", 1, 10).is_err());
        assert_eq!(clean_text("  hi ", "Title", 1, 10).unwrap(), "hi");
        assert!(clean_text("12345678901", "Title", 1, 10).is_err());
        assert_eq!(clean_optional(Some("  "), "Notes", 5).unwrap(), None);
    }
}
