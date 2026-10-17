//! Application launcher: discovery cache, pins, recents, exclusions and safe launching.
//!
//! Application references are stored as either a shell `AppsFolder` parsing name (for
//! everything the Start menu lists, including Store apps) or a validated absolute path to
//! an `.exe`, `.lnk` or `.appref-ms` the user picked. Launching always goes through the
//! shell's default verb with the reference as a *file*, never as a command line.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::util::{now_ms, sha256_hex};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const ICON_SIZE: i32 = 48;
pub const ALLOWED_EXTENSIONS: [&str; 3] = ["exe", "lnk", "appref-ms"];
const MAX_PINS: i64 = 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub source: String,
    pub icon_key: Option<String>,
    pub pinned: bool,
    pub pin_order: Option<i64>,
    pub excluded: bool,
    pub available: bool,
    pub path: Option<String>,
    pub last_used_at: Option<i64>,
    pub launch_count: i64,
}

pub struct DiscoveredApp {
    pub name: String,
    pub parsing_name: String,
    pub icon_key: Option<String>,
}

pub fn shell_id(parsing_name: &str) -> String {
    format!("shell:{parsing_name}")
}

pub fn custom_id(path: &Path) -> String {
    format!("path:{}", path.to_string_lossy().to_lowercase())
}

pub fn icon_key_for(id: &str) -> String {
    sha256_hex(id.as_bytes())[..24].to_string()
}

/// Validates a user-picked application path.
pub fn validate_custom_path(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::invalid("Choose an app using its full path."));
    }
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AppError::invalid("Choose a program (.exe) or a shortcut (.lnk)."));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| AppError::new("missing_path", "That file no longer exists."))?;
    if !canonical.is_file() {
        return Err(AppError::invalid("Choose a program file, not a folder."));
    }
    // `canonicalize` yields a `\\?\` verbatim path on Windows; keep the conventional form.
    let display = canonical.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    Ok(PathBuf::from(display))
}

/// Replaces the discovered (shell) app cache. Pinned, slot-referenced or recently used
/// apps that disappeared are kept but marked unavailable so the UI can explain it.
pub fn merge_discovered(db: &Db, found: &[DiscoveredApp]) -> AppResult<()> {
    let now = now_ms();
    db.tx(|tx| {
        tx.execute("UPDATE applications SET available = 0 WHERE source = 'shell'", [])?;
        for app in found {
            let id = shell_id(&app.parsing_name);
            tx.execute(
                "INSERT INTO applications (id, display_name, source, parsing_name, icon_key, available, updated_at)
                 VALUES (?1, ?2, 'shell', ?3, ?4, 1, ?5)
                 ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name,
                    parsing_name = excluded.parsing_name,
                    icon_key = COALESCE(excluded.icon_key, applications.icon_key),
                    available = 1, updated_at = excluded.updated_at",
                params![id, app.name, app.parsing_name, app.icon_key, now],
            )?;
        }
        tx.execute(
            "DELETE FROM applications WHERE source = 'shell' AND available = 0 AND pinned = 0
               AND id NOT IN (SELECT target FROM dock_slots WHERE utility_type = 'app' AND target IS NOT NULL)
               AND id NOT IN (SELECT application_id FROM app_activity)",
            [],
        )?;
        Ok(())
    })
}

pub fn list(db: &Db) -> AppResult<Vec<AppEntry>> {
    let mut apps = db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT a.id, a.display_name, a.source, a.icon_key, a.pinned, a.pin_order, a.excluded, a.available, a.path,
                    act.last_used_at, COALESCE(act.launch_count, 0)
             FROM applications a LEFT JOIN app_activity act ON act.application_id = a.id
             ORDER BY a.display_name COLLATE NOCASE",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(AppEntry {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    source: r.get(2)?,
                    icon_key: r.get(3)?,
                    pinned: r.get::<_, i64>(4)? != 0,
                    pin_order: r.get(5)?,
                    excluded: r.get::<_, i64>(6)? != 0,
                    available: r.get::<_, i64>(7)? != 0,
                    path: r.get(8)?,
                    last_used_at: r.get(9)?,
                    launch_count: r.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    // Custom paths can be moved or deleted at any time; check on every listing.
    for app in apps.iter_mut().filter(|a| a.source == "custom") {
        app.available = app.path.as_deref().map(|p| Path::new(p).is_file()).unwrap_or(false);
    }
    Ok(apps)
}

#[cfg(test)]
pub fn get(db: &Db, id: &str) -> AppResult<Option<AppEntry>> {
    Ok(list(db)?.into_iter().find(|a| a.id == id))
}

/// The shell target for an app: `shell:AppsFolder\<parsing name>` or the validated path.
pub fn launch_target(db: &Db, id: &str) -> AppResult<(String, Option<PathBuf>)> {
    let row: Option<(String, Option<String>, Option<String>)> = db.with(|c| {
        Ok(c.query_row("SELECT source, parsing_name, path FROM applications WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?)
    })?;
    let (source, parsing, path) = row.ok_or_else(|| AppError::not_found("That app"))?;
    match source.as_str() {
        "shell" => {
            let parsing = parsing.ok_or_else(|| AppError::not_found("That app"))?;
            let available: i64 =
                db.with(|c| Ok(c.query_row("SELECT available FROM applications WHERE id = ?1", [id], |r| r.get(0))?))?;
            if available == 0 {
                return Err(AppError::new("missing_app", "This app is no longer installed. Remove it or pick another app."));
            }
            Ok((format!("shell:AppsFolder\\{parsing}"), None))
        }
        _ => {
            let path = PathBuf::from(path.ok_or_else(|| AppError::not_found("That app"))?);
            if !path.is_file() {
                return Err(AppError::new(
                    "missing_path",
                    format!(
                        "\"{}\" could not be found. It may have been moved or uninstalled.",
                        path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                    ),
                ));
            }
            let dir = path.parent().map(Path::to_path_buf);
            Ok((path.to_string_lossy().to_string(), dir))
        }
    }
}

pub fn record_launch(db: &Db, id: &str, track_recent: bool) -> AppResult<()> {
    if !track_recent {
        return Ok(());
    }
    db.with(|c| {
        let excluded: i64 = c
            .query_row("SELECT excluded FROM applications WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .unwrap_or(1);
        if excluded == 0 {
            c.execute(
                "INSERT INTO app_activity (application_id, last_used_at, launch_count) VALUES (?1, ?2, 1)
                 ON CONFLICT(application_id) DO UPDATE SET last_used_at = excluded.last_used_at,
                    launch_count = app_activity.launch_count + 1",
                params![id, now_ms()],
            )?;
        }
        Ok(())
    })
}

pub fn add_custom(db: &Db, path: &Path, name: &str, icon_key: Option<String>) -> AppResult<String> {
    let id = custom_id(path);
    db.with(|c| {
        c.execute(
            "INSERT INTO applications (id, display_name, source, path, icon_key, pinned, pin_order, available, updated_at)
             VALUES (?1, ?2, 'custom', ?3, ?4, 1, (SELECT COALESCE(MAX(pin_order), 0) + 1 FROM applications), 1, ?5)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, path = excluded.path,
                icon_key = COALESCE(excluded.icon_key, applications.icon_key), pinned = 1, updated_at = excluded.updated_at",
            params![id, name, path.to_string_lossy(), icon_key, now_ms()],
        )?;
        Ok(())
    })?;
    Ok(id)
}

pub fn set_pinned(db: &Db, id: &str, pinned: bool) -> AppResult<()> {
    db.tx(|tx| {
        if pinned {
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM applications WHERE pinned = 1 AND id != ?1", [id], |r| r.get(0))?;
            if count >= MAX_PINS {
                return Err(AppError::invalid(format!("You can pin up to {MAX_PINS} apps.")));
            }
        }
        let n = tx.execute(
            "UPDATE applications SET pinned = ?2,
                pin_order = CASE WHEN ?2 = 1 THEN (SELECT COALESCE(MAX(pin_order), 0) + 1 FROM applications) ELSE NULL END
             WHERE id = ?1",
            params![id, pinned as i64],
        )?;
        if n == 0 {
            return Err(AppError::not_found("That app"));
        }
        Ok(())
    })
}

pub fn reorder_pins(db: &Db, ids: &[String]) -> AppResult<()> {
    db.tx(|tx| {
        for (i, id) in ids.iter().enumerate() {
            tx.execute("UPDATE applications SET pin_order = ?2 WHERE id = ?1 AND pinned = 1", params![id, i as i64 + 1])?;
        }
        Ok(())
    })
}

pub fn set_excluded(db: &Db, id: &str, excluded: bool) -> AppResult<()> {
    db.tx(|tx| {
        let n = tx.execute("UPDATE applications SET excluded = ?2 WHERE id = ?1", params![id, excluded as i64])?;
        if n == 0 {
            return Err(AppError::not_found("That app"));
        }
        if excluded {
            tx.execute("DELETE FROM app_activity WHERE application_id = ?1", [id])?;
        }
        Ok(())
    })
}

/// Removes a custom app entirely, or unpins a discovered app.
pub fn remove(db: &Db, id: &str) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM applications WHERE id = ?1 AND source = 'custom'", [id])?;
        c.execute("UPDATE applications SET pinned = 0, pin_order = NULL WHERE id = ?1", [id])?;
        c.execute("DELETE FROM app_activity WHERE application_id = ?1", [id])?;
        Ok(())
    })
}

pub fn clear_recent(db: &Db) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM app_activity", [])?;
        Ok(())
    })
}

/// Resets launcher data: pins, exclusions, recents and custom apps.
pub fn reset(db: &Db) -> AppResult<()> {
    db.tx(|tx| {
        tx.execute("DELETE FROM app_activity", [])?;
        tx.execute("DELETE FROM applications WHERE source = 'custom'", [])?;
        tx.execute("UPDATE applications SET pinned = 0, pin_order = NULL, excluded = 0", [])?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovered(names: &[&str]) -> Vec<DiscoveredApp> {
        names
            .iter()
            .map(|n| DiscoveredApp { name: n.to_string(), parsing_name: format!("{n}.App"), icon_key: None })
            .collect()
    }

    #[test]
    fn merge_keeps_pinned_missing_apps_and_drops_others() {
        let db = Db::open_in_memory().unwrap();
        merge_discovered(&db, &discovered(&["Alpha", "Beta", "Gamma"])).unwrap();
        set_pinned(&db, &shell_id("Beta.App"), true).unwrap();
        merge_discovered(&db, &discovered(&["Alpha"])).unwrap();
        let apps = list(&db).unwrap();
        assert_eq!(apps.len(), 2);
        let beta = apps.iter().find(|a| a.name == "Beta").unwrap();
        assert!(beta.pinned && !beta.available);
        assert!(launch_target(&db, &beta.id).is_err());
        let alpha = apps.iter().find(|a| a.name == "Alpha").unwrap();
        assert_eq!(launch_target(&db, &alpha.id).unwrap().0, "shell:AppsFolder\\Alpha.App");
    }

    #[test]
    fn recents_respect_exclusion_and_tracking() {
        let db = Db::open_in_memory().unwrap();
        merge_discovered(&db, &discovered(&["Alpha", "Beta"])).unwrap();
        let a = shell_id("Alpha.App");
        let b = shell_id("Beta.App");
        record_launch(&db, &a, true).unwrap();
        record_launch(&db, &a, true).unwrap();
        record_launch(&db, &b, false).unwrap();
        set_excluded(&db, &b, true).unwrap();
        record_launch(&db, &b, true).unwrap();
        let apps = list(&db).unwrap();
        assert_eq!(apps.iter().find(|x| x.id == a).unwrap().launch_count, 2);
        assert_eq!(apps.iter().find(|x| x.id == b).unwrap().launch_count, 0);
        clear_recent(&db).unwrap();
        assert!(list(&db).unwrap().iter().all(|x| x.last_used_at.is_none()));
    }

    #[test]
    fn custom_paths_are_validated_and_detected_missing() {
        let dir = std::env::temp_dir().join(format!("wisp-apps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("tool.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        assert!(validate_custom_path(Path::new("relative.exe")).is_err());
        assert!(validate_custom_path(&dir.join("notes.txt")).is_err());
        let valid = validate_custom_path(&exe).unwrap();
        assert!(!valid.to_string_lossy().starts_with(r"\\?\"));

        let db = Db::open_in_memory().unwrap();
        let id = add_custom(&db, &valid, "Tool", None).unwrap();
        assert!(get(&db, &id).unwrap().unwrap().available);
        std::fs::remove_file(&exe).unwrap();
        assert!(!get(&db, &id).unwrap().unwrap().available);
        let err = launch_target(&db, &id).unwrap_err();
        assert_eq!(err.code, "missing_path");
        remove(&db, &id).unwrap();
        assert!(get(&db, &id).unwrap().is_none());
    }

    #[test]
    fn reset_clears_launcher_data() {
        let db = Db::open_in_memory().unwrap();
        merge_discovered(&db, &discovered(&["Alpha"])).unwrap();
        let a = shell_id("Alpha.App");
        set_pinned(&db, &a, true).unwrap();
        set_excluded(&db, &a, true).unwrap();
        reset(&db).unwrap();
        let app = get(&db, &a).unwrap().unwrap();
        assert!(!app.pinned && !app.excluded);
    }
}
