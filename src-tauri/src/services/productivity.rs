//! Tasks, reminders and local calendar events.
//!
//! Reminder delivery is at-most-once: a due reminder is atomically *claimed*
//! (`scheduled` -> `notified`) in the database before any notification is shown, so a
//! restart, a sleep/wake cycle or a second scheduler pass can never notify twice.
//! Reminders that were due while the app was not running are delivered once on start if
//! they are less than `MISSED_GRACE_MS` old; older ones are marked `missed` silently.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::util::{clean_optional, clean_text, now_ms};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

pub const MISSED_GRACE_MS: i64 = 24 * 60 * 60 * 1000;
/// A reminder's attached app is only auto-launched if the reminder fires within this window.
pub const LAUNCH_GRACE_MS: i64 = 10 * 60 * 1000;

// ---------- Tasks ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub notes: Option<String>,
    pub completed: bool,
    pub completed_at: Option<i64>,
    pub priority: i64,
    pub group_name: Option<String>,
    pub due_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub title: String,
    pub notes: Option<String>,
    pub priority: Option<i64>,
    pub group_name: Option<String>,
    pub due_at: Option<i64>,
}

/// Distinguishes a missing field (`None`) from an explicit `null` (`Some(None)`).
fn double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TaskPatch {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub notes: Option<Option<String>>,
    pub completed: Option<bool>,
    pub priority: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    pub group_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_at: Option<Option<i64>>,
}

fn task_from(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        title: r.get(1)?,
        notes: r.get(2)?,
        completed: r.get::<_, i64>(3)? != 0,
        completed_at: r.get(4)?,
        priority: r.get(5)?,
        group_name: r.get(6)?,
        due_at: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

const TASK_COLS: &str = "id, title, notes, completed, completed_at, priority, group_name, due_at, created_at, updated_at";

fn check_priority(p: i64) -> AppResult<i64> {
    if (0..=3).contains(&p) { Ok(p) } else { Err(AppError::invalid("Priority must be none, low, medium or high.")) }
}

pub fn list_tasks(db: &Db) -> AppResult<Vec<Task>> {
    db.with(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {TASK_COLS} FROM tasks
             ORDER BY completed ASC, CASE WHEN due_at IS NULL THEN 1 ELSE 0 END, due_at ASC, priority DESC, id DESC"
        ))?;
        let rows = stmt.query_map([], task_from)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn get_task(db: &Db, id: i64) -> AppResult<Task> {
    db.with(|c| {
        c.query_row(&format!("SELECT {TASK_COLS} FROM tasks WHERE id = ?1"), [id], task_from)
            .optional()?
            .ok_or_else(|| AppError::not_found("That task"))
    })
}

pub fn create_task(db: &Db, input: &TaskInput) -> AppResult<Task> {
    let title = clean_text(&input.title, "Task title", 1, 200)?;
    let notes = clean_optional(input.notes.as_deref(), "Notes", 2000)?;
    let group = clean_optional(input.group_name.as_deref(), "Group", 40)?;
    let priority = check_priority(input.priority.unwrap_or(0))?;
    let now = now_ms();
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO tasks (title, notes, priority, group_name, due_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![title, notes, priority, group, input.due_at, now],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    get_task(db, id)
}

pub fn update_task(db: &Db, id: i64, patch: &TaskPatch) -> AppResult<Task> {
    let mut t = get_task(db, id)?;
    if let Some(title) = &patch.title {
        t.title = clean_text(title, "Task title", 1, 200)?;
    }
    if let Some(notes) = &patch.notes {
        t.notes = clean_optional(notes.as_deref(), "Notes", 2000)?;
    }
    if let Some(group) = &patch.group_name {
        t.group_name = clean_optional(group.as_deref(), "Group", 40)?;
    }
    if let Some(p) = patch.priority {
        t.priority = check_priority(p)?;
    }
    if let Some(due) = patch.due_at {
        t.due_at = due;
    }
    let now = now_ms();
    if let Some(done) = patch.completed {
        if done != t.completed {
            t.completed = done;
            t.completed_at = if done { Some(now) } else { None };
        }
    }
    db.with(|c| {
        c.execute(
            "UPDATE tasks SET title = ?2, notes = ?3, completed = ?4, completed_at = ?5, priority = ?6,
                group_name = ?7, due_at = ?8, updated_at = ?9 WHERE id = ?1",
            params![id, t.title, t.notes, t.completed as i64, t.completed_at, t.priority, t.group_name, t.due_at, now],
        )?;
        Ok(())
    })?;
    get_task(db, id)
}

pub fn delete_task(db: &Db, id: i64) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(())
    })
}

pub fn clear_completed_tasks(db: &Db) -> AppResult<usize> {
    db.with(|c| Ok(c.execute("DELETE FROM tasks WHERE completed = 1", [])?))
}

// ---------- Reminders ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: i64,
    pub title: String,
    pub scheduled_at: i64,
    pub notification_state: String,
    pub notified_at: Option<i64>,
    pub dismissed_at: Option<i64>,
    pub launch_app_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderInput {
    pub title: String,
    pub scheduled_at: i64,
    pub launch_app_id: Option<String>,
}

const REMINDER_COLS: &str =
    "id, title, scheduled_at, notification_state, notified_at, dismissed_at, launch_app_id, created_at, updated_at";

fn reminder_from(r: &Row) -> rusqlite::Result<Reminder> {
    Ok(Reminder {
        id: r.get(0)?,
        title: r.get(1)?,
        scheduled_at: r.get(2)?,
        notification_state: r.get(3)?,
        notified_at: r.get(4)?,
        dismissed_at: r.get(5)?,
        launch_app_id: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

pub fn list_reminders(db: &Db) -> AppResult<Vec<Reminder>> {
    db.with(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {REMINDER_COLS} FROM reminders
             ORDER BY CASE notification_state WHEN 'dismissed' THEN 1 ELSE 0 END, scheduled_at ASC"
        ))?;
        let rows = stmt.query_map([], reminder_from)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn get_reminder(db: &Db, id: i64) -> AppResult<Reminder> {
    db.with(|c| {
        c.query_row(&format!("SELECT {REMINDER_COLS} FROM reminders WHERE id = ?1"), [id], reminder_from)
            .optional()?
            .ok_or_else(|| AppError::not_found("That reminder"))
    })
}

fn check_launch_app(db: &Db, app: Option<&str>) -> AppResult<Option<String>> {
    match app.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(id) => {
            let n: i64 = db.with(|c| Ok(c.query_row("SELECT COUNT(*) FROM applications WHERE id = ?1", [id], |r| r.get(0))?))?;
            if n == 0 {
                return Err(AppError::not_found("That app"));
            }
            Ok(Some(id.to_string()))
        }
    }
}

pub fn create_reminder(db: &Db, input: &ReminderInput, now: i64) -> AppResult<Reminder> {
    let title = clean_text(&input.title, "Reminder", 1, 200)?;
    if input.scheduled_at < now - 60_000 {
        return Err(AppError::invalid("Choose a time in the future."));
    }
    let app = check_launch_app(db, input.launch_app_id.as_deref())?;
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO reminders (title, scheduled_at, notification_state, launch_app_id, created_at, updated_at)
             VALUES (?1, ?2, 'scheduled', ?3, ?4, ?4)",
            params![title, input.scheduled_at, app, now],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    get_reminder(db, id)
}

/// Edits a reminder. Changing the time re-arms it (explicit reschedule).
pub fn update_reminder(db: &Db, id: i64, input: &ReminderInput, now: i64) -> AppResult<Reminder> {
    let current = get_reminder(db, id)?;
    let title = clean_text(&input.title, "Reminder", 1, 200)?;
    let app = check_launch_app(db, input.launch_app_id.as_deref())?;
    let rescheduled = input.scheduled_at != current.scheduled_at;
    if rescheduled && input.scheduled_at < now - 60_000 {
        return Err(AppError::invalid("Choose a time in the future."));
    }
    db.with(|c| {
        if rescheduled {
            c.execute(
                "UPDATE reminders SET title = ?2, scheduled_at = ?3, launch_app_id = ?4, notification_state = 'scheduled',
                    notified_at = NULL, dismissed_at = NULL, updated_at = ?5 WHERE id = ?1",
                params![id, title, input.scheduled_at, app, now],
            )?;
        } else {
            c.execute(
                "UPDATE reminders SET title = ?2, launch_app_id = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, title, app, now],
            )?;
        }
        Ok(())
    })?;
    get_reminder(db, id)
}

pub fn dismiss_reminder(db: &Db, id: i64, now: i64) -> AppResult<Reminder> {
    let n = db.with(|c| {
        Ok(c.execute(
            "UPDATE reminders SET notification_state = 'dismissed', dismissed_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?)
    })?;
    if n == 0 {
        return Err(AppError::not_found("That reminder"));
    }
    get_reminder(db, id)
}

pub fn delete_reminder(db: &Db, id: i64) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM reminders WHERE id = ?1", [id])?;
        Ok(())
    })
}

pub fn clear_dismissed_reminders(db: &Db) -> AppResult<usize> {
    db.with(|c| Ok(c.execute("DELETE FROM reminders WHERE notification_state = 'dismissed'", [])?))
}

#[derive(Debug, PartialEq)]
pub enum Delivery {
    /// Show a notification (and maybe launch the attached app).
    Notify { reminder: Reminder, late_by_ms: i64, launch_app: Option<String> },
    /// Too old to notify; marked as missed.
    Missed(Reminder),
}

/// Claims every due reminder exactly once and returns what to deliver.
pub fn claim_due(db: &Db, now: i64) -> AppResult<Vec<Delivery>> {
    let due: Vec<Reminder> = db.with(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {REMINDER_COLS} FROM reminders WHERE notification_state = 'scheduled' AND scheduled_at <= ?1
             ORDER BY scheduled_at"
        ))?;
        let rows = stmt.query_map([now], reminder_from)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    let mut out = Vec::new();
    for r in due {
        let late = now - r.scheduled_at;
        let next_state = if late > MISSED_GRACE_MS { "missed" } else { "notified" };
        let claimed = db.with(|c| {
            Ok(c.execute(
                "UPDATE reminders SET notification_state = ?2, notified_at = ?3, updated_at = ?3
                 WHERE id = ?1 AND notification_state = 'scheduled'",
                params![r.id, next_state, now],
            )?)
        })?;
        if claimed != 1 {
            continue; // someone else claimed it
        }
        let mut claimed_r = r.clone();
        claimed_r.notification_state = next_state.into();
        claimed_r.notified_at = Some(now);
        if next_state == "missed" {
            out.push(Delivery::Missed(claimed_r));
        } else {
            let launch_app = if late <= LAUNCH_GRACE_MS { r.launch_app_id.clone() } else { None };
            out.push(Delivery::Notify { reminder: claimed_r, late_by_ms: late, launch_app });
        }
    }
    Ok(out)
}

/// Earliest pending reminder time, used to schedule the next wake-up.
pub fn next_due_at(db: &Db) -> AppResult<Option<i64>> {
    db.with(|c| {
        Ok(c.query_row("SELECT MIN(scheduled_at) FROM reminders WHERE notification_state = 'scheduled'", [], |r| r.get(0))?)
    })
}

// ---------- Calendar ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: i64,
    pub title: String,
    pub start_at: i64,
    pub end_at: i64,
    pub all_day: bool,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventInput {
    pub title: String,
    pub start_at: i64,
    pub end_at: i64,
    #[serde(default)]
    pub all_day: bool,
    pub description: Option<String>,
}

const EVENT_COLS: &str = "id, title, start_at, end_at, all_day, description, created_at, updated_at";

fn event_from(r: &Row) -> rusqlite::Result<CalendarEvent> {
    Ok(CalendarEvent {
        id: r.get(0)?,
        title: r.get(1)?,
        start_at: r.get(2)?,
        end_at: r.get(3)?,
        all_day: r.get::<_, i64>(4)? != 0,
        description: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

fn validate_event(input: &EventInput) -> AppResult<(String, Option<String>)> {
    let title = clean_text(&input.title, "Event title", 1, 200)?;
    let description = clean_optional(input.description.as_deref(), "Description", 2000)?;
    if input.end_at < input.start_at {
        return Err(AppError::invalid("The end time must be after the start time."));
    }
    if input.end_at - input.start_at > 366 * 86_400_000 {
        return Err(AppError::invalid("Events can be at most one year long."));
    }
    Ok((title, description))
}

/// Events overlapping `[from, to)`.
pub fn list_events(db: &Db, from: i64, to: i64) -> AppResult<Vec<CalendarEvent>> {
    if to <= from {
        return Ok(vec![]);
    }
    db.with(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {EVENT_COLS} FROM calendar_events WHERE start_at < ?2 AND end_at >= ?1 ORDER BY start_at, id"
        ))?;
        let rows = stmt.query_map(params![from, to], event_from)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn get_event(db: &Db, id: i64) -> AppResult<CalendarEvent> {
    db.with(|c| {
        c.query_row(&format!("SELECT {EVENT_COLS} FROM calendar_events WHERE id = ?1"), [id], event_from)
            .optional()?
            .ok_or_else(|| AppError::not_found("That event"))
    })
}

pub fn create_event(db: &Db, input: &EventInput) -> AppResult<CalendarEvent> {
    let (title, description) = validate_event(input)?;
    let now = now_ms();
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO calendar_events (title, start_at, end_at, all_day, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![title, input.start_at, input.end_at, input.all_day as i64, description, now],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    get_event(db, id)
}

pub fn update_event(db: &Db, id: i64, input: &EventInput) -> AppResult<CalendarEvent> {
    let (title, description) = validate_event(input)?;
    let n = db.with(|c| {
        Ok(c.execute(
            "UPDATE calendar_events SET title = ?2, start_at = ?3, end_at = ?4, all_day = ?5, description = ?6, updated_at = ?7
             WHERE id = ?1",
            params![id, title, input.start_at, input.end_at, input.all_day as i64, description, now_ms()],
        )?)
    })?;
    if n == 0 {
        return Err(AppError::not_found("That event"));
    }
    get_event(db, id)
}

pub fn delete_event(db: &Db, id: i64) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM calendar_events WHERE id = ?1", [id])?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_lifecycle() {
        let db = Db::open_in_memory().unwrap();
        assert!(create_task(&db, &TaskInput { title: "  ".into(), ..Default::default() }).is_err());
        assert!(create_task(&db, &TaskInput { title: "x".into(), priority: Some(9), ..Default::default() }).is_err());
        let t = create_task(&db, &TaskInput { title: " Write PRD ".into(), priority: Some(2), ..Default::default() }).unwrap();
        assert_eq!(t.title, "Write PRD");
        let done = update_task(&db, t.id, &TaskPatch { completed: Some(true), ..Default::default() }).unwrap();
        assert!(done.completed && done.completed_at.is_some());
        let undone = update_task(&db, t.id, &TaskPatch { completed: Some(false), due_at: Some(Some(5)), ..Default::default() }).unwrap();
        assert!(!undone.completed && undone.completed_at.is_none());
        assert_eq!(undone.due_at, Some(5));
        let cleared = update_task(&db, t.id, &TaskPatch { due_at: Some(None), ..Default::default() }).unwrap();
        assert_eq!(cleared.due_at, None);
        update_task(&db, t.id, &TaskPatch { completed: Some(true), ..Default::default() }).unwrap();
        assert_eq!(clear_completed_tasks(&db).unwrap(), 1);
        assert!(list_tasks(&db).unwrap().is_empty());
    }

    #[test]
    fn task_patch_distinguishes_null_from_missing() {
        let p: TaskPatch = serde_json::from_str(r#"{"dueAt": null}"#).unwrap();
        assert_eq!(p.due_at, Some(None));
        let p: TaskPatch = serde_json::from_str(r#"{"dueAt": 5}"#).unwrap();
        assert_eq!(p.due_at, Some(Some(5)));
        let p: TaskPatch = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(p.due_at, None);
    }

    #[test]
    fn reminders_notify_exactly_once_across_restarts() {
        let db = Db::open_in_memory().unwrap();
        let now = 1_000_000_000;
        let r = create_reminder(&db, &ReminderInput { title: "Stand up".into(), scheduled_at: now + 1000, launch_app_id: None }, now).unwrap();
        assert!(claim_due(&db, now).unwrap().is_empty());
        assert_eq!(next_due_at(&db).unwrap(), Some(now + 1000));
        let first = claim_due(&db, now + 1500).unwrap();
        assert!(matches!(&first[..], [Delivery::Notify { late_by_ms: 500, .. }]));
        // A second pass (or a restart, which runs the same code) delivers nothing.
        assert!(claim_due(&db, now + 2000).unwrap().is_empty());
        assert_eq!(next_due_at(&db).unwrap(), None);
        // Dismissing keeps it quiet.
        dismiss_reminder(&db, r.id, now + 3000).unwrap();
        assert!(claim_due(&db, now + 10_000_000).unwrap().is_empty());
        // Explicit reschedule re-arms it.
        update_reminder(&db, r.id, &ReminderInput { title: "Stand up".into(), scheduled_at: now + 20_000, launch_app_id: None }, now + 4000)
            .unwrap();
        assert_eq!(claim_due(&db, now + 20_000).unwrap().len(), 1);
    }

    #[test]
    fn stale_reminders_are_marked_missed_without_notification() {
        let db = Db::open_in_memory().unwrap();
        let now = 10 * MISSED_GRACE_MS;
        create_reminder(&db, &ReminderInput { title: "Old".into(), scheduled_at: now, launch_app_id: None }, now).unwrap();
        let out = claim_due(&db, now + MISSED_GRACE_MS + 1).unwrap();
        assert!(matches!(&out[..], [Delivery::Missed(r)] if r.notification_state == "missed"));
    }

    #[test]
    fn reminder_validation() {
        let db = Db::open_in_memory().unwrap();
        let now = 1_000_000;
        assert!(create_reminder(&db, &ReminderInput { title: "x".into(), scheduled_at: now - 120_000, launch_app_id: None }, now).is_err());
        assert!(create_reminder(&db, &ReminderInput { title: "x".into(), scheduled_at: now + 1, launch_app_id: Some("shell:none".into()) }, now).is_err());
    }

    #[test]
    fn calendar_range_and_validation() {
        let db = Db::open_in_memory().unwrap();
        let e = |t: &str, s: i64, en: i64| EventInput { title: t.into(), start_at: s, end_at: en, all_day: false, description: None };
        assert!(create_event(&db, &e("bad", 10, 5)).is_err());
        create_event(&db, &e("a", 0, 10)).unwrap();
        create_event(&db, &e("b", 50, 60)).unwrap();
        let spans = create_event(&db, &e("c", 5, 100)).unwrap();
        let in_range: Vec<String> = list_events(&db, 40, 55).unwrap().into_iter().map(|x| x.title).collect();
        assert_eq!(in_range, ["c", "b"]);
        update_event(&db, spans.id, &e("c2", 5, 6)).unwrap();
        assert_eq!(list_events(&db, 40, 55).unwrap().len(), 1);
        delete_event(&db, spans.id).unwrap();
        assert_eq!(list_events(&db, 0, 1000).unwrap().len(), 2);
    }
}
