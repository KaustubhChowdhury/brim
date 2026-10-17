//! The four configurable quick-access slots.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::settings::Utility;
use rusqlite::params;
use serde::{Deserialize, Serialize};

pub const SLOT_COUNT: usize = 4;
pub const DEFAULT_SLOTS: [Utility; SLOT_COUNT] = [Utility::Music, Utility::Clipboard, Utility::Apps, Utility::Tasks];

/// Icons a utility slot may use (names match the frontend icon set).
pub const ALLOWED_ICONS: [&str; 20] = [
    "music", "headphones", "clipboard", "clipboard-list", "layout-grid", "rocket", "check-square", "list-todo",
    "calendar", "calendar-days", "code", "file-text", "image", "palette", "star", "heart", "bolt", "bookmark",
    "terminal", "sparkles",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlotAssignment {
    /// A utility name (`music`, `clipboard`, …) or `app`.
    pub kind: String,
    /// Application id when `kind == "app"`.
    pub target: Option<String>,
    pub label: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotView {
    pub slot_number: usize,
    pub kind: String,
    pub target: Option<String>,
    pub label: String,
    pub custom_label: Option<String>,
    pub icon: Option<String>,
    /// `ok`, `missing` (app gone) or `invalid` (unknown utility).
    pub status: String,
    pub status_message: Option<String>,
}

fn default_assignment(slot: usize) -> SlotAssignment {
    SlotAssignment { kind: DEFAULT_SLOTS[slot].as_str().into(), target: None, label: None, icon: None }
}

pub fn validate(a: &SlotAssignment) -> AppResult<SlotAssignment> {
    let label = match a.label.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(l) if l.chars().count() > 24 => return Err(AppError::invalid("Slot labels must be 24 characters or fewer.")),
        Some(l) => Some(l.to_string()),
    };
    let icon = match a.icon.as_deref() {
        None | Some("") => None,
        Some(i) if ALLOWED_ICONS.contains(&i) => Some(i.to_string()),
        Some(_) => return Err(AppError::invalid("That icon is not available.")),
    };
    if a.kind == "app" {
        let target = a.target.as_deref().map(str::trim).filter(|t| !t.is_empty());
        let Some(target) = target else { return Err(AppError::invalid("Choose an app for this slot.")) };
        return Ok(SlotAssignment { kind: "app".into(), target: Some(target.to_string()), label, icon: None });
    }
    if Utility::parse(&a.kind).is_none() {
        return Err(AppError::invalid("That utility is not available."));
    }
    Ok(SlotAssignment { kind: a.kind.clone(), target: None, label, icon })
}

fn read_raw(db: &Db) -> AppResult<Vec<SlotAssignment>> {
    let mut slots: Vec<SlotAssignment> = (0..SLOT_COUNT).map(default_assignment).collect();
    db.with(|c| {
        let mut stmt = c.prepare("SELECT slot_number, utility_type, target, label, icon FROM dock_slots")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, SlotAssignment { kind: r.get(1)?, target: r.get(2)?, label: r.get(3)?, icon: r.get(4)? }))
        })?;
        for row in rows {
            let (n, a) = row?;
            if (0..SLOT_COUNT as i64).contains(&n) {
                slots[n as usize] = a;
            }
        }
        Ok(())
    })?;
    Ok(slots)
}

/// Slots with validation status, so the UI can explain and offer to fix broken entries.
pub fn list(db: &Db) -> AppResult<Vec<SlotView>> {
    let apps = crate::services::apps::list(db)?;
    let raw = read_raw(db)?;
    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(i, a)| {
            let (label, status, message) = if a.kind == "app" {
                match a.target.as_deref().and_then(|t| apps.iter().find(|x| x.id == t)) {
                    Some(app) if app.available => (app.name.clone(), "ok", None),
                    Some(app) => (
                        app.name.clone(),
                        "missing",
                        Some(format!("{} is no longer available. Choose another app or utility.", app.name)),
                    ),
                    None => ("Missing app".into(), "missing", Some("This slot's app could not be found.".into())),
                }
            } else if let Some(u) = Utility::parse(&a.kind) {
                (u.label().to_string(), "ok", None)
            } else {
                ("Unknown".into(), "invalid", Some("This slot refers to a utility that is not available.".into()))
            };
            SlotView {
                slot_number: i,
                kind: a.kind,
                target: a.target,
                label: a.label.clone().unwrap_or(label),
                custom_label: a.label,
                icon: a.icon,
                status: status.into(),
                status_message: message,
            }
        })
        .collect())
}

pub fn set(db: &Db, slot: usize, assignment: &SlotAssignment) -> AppResult<()> {
    if slot >= SLOT_COUNT {
        return Err(AppError::invalid("There are only four slots."));
    }
    let a = validate(assignment)?;
    if let Some(target) = &a.target {
        let exists: i64 =
            db.with(|c| Ok(c.query_row("SELECT COUNT(*) FROM applications WHERE id = ?1", [target], |r| r.get(0))?))?;
        if exists == 0 {
            return Err(AppError::not_found("That app"));
        }
    }
    db.with(|c| {
        c.execute(
            "INSERT INTO dock_slots (slot_number, utility_type, target, label, icon) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(slot_number) DO UPDATE SET utility_type = excluded.utility_type, target = excluded.target,
                label = excluded.label, icon = excluded.icon",
            params![slot as i64, a.kind, a.target, a.label, a.icon],
        )?;
        Ok(())
    })
}

/// Moves a slot to a new position, shifting the others.
pub fn move_slot(db: &Db, from: usize, to: usize) -> AppResult<()> {
    if from >= SLOT_COUNT || to >= SLOT_COUNT {
        return Err(AppError::invalid("There are only four slots."));
    }
    let mut raw = read_raw(db)?;
    let item = raw.remove(from);
    raw.insert(to, item);
    write_all(db, &raw)
}

fn write_all(db: &Db, slots: &[SlotAssignment]) -> AppResult<()> {
    db.tx(|tx| {
        tx.execute("DELETE FROM dock_slots", [])?;
        for (i, a) in slots.iter().enumerate() {
            tx.execute(
                "INSERT INTO dock_slots (slot_number, utility_type, target, label, icon) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![i as i64, a.kind, a.target, a.label, a.icon],
            )?;
        }
        Ok(())
    })
}

pub fn reset(db: &Db) -> AppResult<()> {
    let defaults: Vec<SlotAssignment> = (0..SLOT_COUNT).map(default_assignment).collect();
    write_all(db, &defaults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::apps;

    #[test]
    fn defaults_then_assign_move_and_reset() {
        let db = Db::open_in_memory().unwrap();
        let views = list(&db).unwrap();
        assert_eq!(views.len(), 4);
        assert_eq!(views.iter().map(|v| v.kind.as_str()).collect::<Vec<_>>(), ["music", "clipboard", "apps", "tasks"]);

        set(&db, 3, &SlotAssignment { kind: "snippets".into(), target: None, label: Some(" Code ".into()), icon: Some("code".into()) })
            .unwrap();
        let v = &list(&db).unwrap()[3];
        assert_eq!((v.kind.as_str(), v.label.as_str(), v.icon.as_deref()), ("snippets", "Code", Some("code")));

        move_slot(&db, 3, 0).unwrap();
        let kinds: Vec<String> = list(&db).unwrap().into_iter().map(|v| v.kind).collect();
        assert_eq!(kinds, ["snippets", "music", "clipboard", "apps"]);

        reset(&db).unwrap();
        assert_eq!(list(&db).unwrap()[0].kind, "music");
    }

    #[test]
    fn invalid_assignments_rejected() {
        let db = Db::open_in_memory().unwrap();
        let bad_kind = SlotAssignment { kind: "ai".into(), target: None, label: None, icon: None };
        assert!(set(&db, 0, &bad_kind).is_err());
        let no_app = SlotAssignment { kind: "app".into(), target: Some("shell:nope".into()), label: None, icon: None };
        assert!(set(&db, 0, &no_app).is_err());
        let bad_icon = SlotAssignment { kind: "music".into(), target: None, label: None, icon: Some("../x".into()) };
        assert!(set(&db, 0, &bad_icon).is_err());
        assert!(set(&db, 4, &default_assignment(0)).is_err());
    }

    #[test]
    fn app_slot_reports_missing_app() {
        let db = Db::open_in_memory().unwrap();
        apps::merge_discovered(&db, &[apps::DiscoveredApp { name: "Alpha".into(), parsing_name: "A".into(), icon_key: None }])
            .unwrap();
        let id = apps::shell_id("A");
        set(&db, 1, &SlotAssignment { kind: "app".into(), target: Some(id.clone()), label: None, icon: None }).unwrap();
        assert_eq!(list(&db).unwrap()[1].status, "ok");
        apps::merge_discovered(&db, &[]).unwrap(); // uninstalled; kept because a slot references it
        let v = &list(&db).unwrap()[1];
        assert_eq!(v.status, "missing");
        assert!(v.status_message.is_some());
    }
}
