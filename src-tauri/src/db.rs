//! Local SQLite storage with versioned, transactional, non-destructive migrations.
//!
//! The schema version lives in `PRAGMA user_version`. Each migration runs in its own
//! transaction together with the version bump, so a failed migration leaves the
//! previous version intact. Before migrating an existing database a snapshot copy is
//! written next to it (`wisp.db.v<N>.bak`) so user data can always be recovered.

use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use rusqlite::{Connection, Transaction};
use std::path::Path;

/// Ordered list of migrations. Index 0 upgrades version 0 -> 1, and so on.
/// Never edit a shipped migration; append a new one instead.
pub const MIGRATIONS: &[&str] = &[
    // v1 — dock foundation and first usable release (roadmap phases 1–2).
    r#"
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    CREATE TABLE dock_slots (
        slot_number  INTEGER PRIMARY KEY CHECK (slot_number BETWEEN 0 AND 3),
        utility_type TEXT NOT NULL,
        target       TEXT,
        label        TEXT,
        icon         TEXT
    );
    CREATE TABLE clipboard_items (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        content      TEXT NOT NULL,
        content_type TEXT NOT NULL DEFAULT 'text',
        content_hash TEXT NOT NULL UNIQUE,
        created_at   INTEGER NOT NULL,
        last_used_at INTEGER NOT NULL,
        pinned       INTEGER NOT NULL DEFAULT 0,
        size_bytes   INTEGER NOT NULL,
        copy_count   INTEGER NOT NULL DEFAULT 1
    );
    CREATE INDEX idx_clipboard_recency ON clipboard_items (last_used_at DESC, id DESC);
    CREATE TABLE applications (
        id            TEXT PRIMARY KEY,
        display_name  TEXT NOT NULL,
        source        TEXT NOT NULL CHECK (source IN ('shell', 'custom')),
        parsing_name  TEXT,
        path          TEXT,
        icon_key      TEXT,
        pinned        INTEGER NOT NULL DEFAULT 0,
        pin_order     INTEGER,
        excluded      INTEGER NOT NULL DEFAULT 0,
        available     INTEGER NOT NULL DEFAULT 1,
        updated_at    INTEGER NOT NULL
    );
    CREATE TABLE app_activity (
        application_id TEXT PRIMARY KEY REFERENCES applications (id) ON DELETE CASCADE,
        last_used_at   INTEGER NOT NULL,
        launch_count   INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE snippets (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        title        TEXT NOT NULL,
        content      TEXT NOT NULL,
        category     TEXT,
        created_at   INTEGER NOT NULL,
        updated_at   INTEGER NOT NULL,
        last_used_at INTEGER
    );
    "#,
    // v2 — media/appearance and productivity modules (roadmap phases 3–4).
    r#"
    CREATE TABLE wallpapers (
        id             INTEGER PRIMARY KEY AUTOINCREMENT,
        path           TEXT NOT NULL UNIQUE,
        title          TEXT NOT NULL,
        favourite      INTEGER NOT NULL DEFAULT 0,
        thumbnail_path TEXT,
        width          INTEGER,
        height         INTEGER,
        created_at     INTEGER NOT NULL
    );
    CREATE TABLE tasks (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        title        TEXT NOT NULL,
        notes        TEXT,
        completed    INTEGER NOT NULL DEFAULT 0,
        completed_at INTEGER,
        priority     INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN 0 AND 3),
        group_name   TEXT,
        due_at       INTEGER,
        created_at   INTEGER NOT NULL,
        updated_at   INTEGER NOT NULL
    );
    CREATE TABLE reminders (
        id                 INTEGER PRIMARY KEY AUTOINCREMENT,
        title              TEXT NOT NULL,
        scheduled_at       INTEGER NOT NULL,
        notification_state TEXT NOT NULL DEFAULT 'scheduled'
                           CHECK (notification_state IN ('scheduled', 'notified', 'missed', 'dismissed')),
        notified_at        INTEGER,
        dismissed_at       INTEGER,
        launch_app_id      TEXT,
        created_at         INTEGER NOT NULL,
        updated_at         INTEGER NOT NULL
    );
    CREATE INDEX idx_reminders_due ON reminders (notification_state, scheduled_at);
    CREATE TABLE calendar_events (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        title       TEXT NOT NULL,
        start_at    INTEGER NOT NULL,
        end_at      INTEGER NOT NULL,
        all_day     INTEGER NOT NULL DEFAULT 0,
        description TEXT,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL,
        CHECK (end_at >= start_at)
    );
    CREATE INDEX idx_calendar_range ON calendar_events (start_at, end_at);
    "#,
];

pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        configure(&conn)?;
        let version = user_version(&conn)?;
        if version > LATEST_VERSION {
            return Err(AppError::new(
                "storage_newer",
                "Your data was created by a newer version of Brim. Please update the app; your data has not been changed.",
            )
            .with_detail(format!("db version {version} > supported {LATEST_VERSION}")));
        }
        if version > 0 && version < LATEST_VERSION {
            backup(&conn, path, version)?;
        }
        migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        configure(&conn)?;
        migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Runs `f` with exclusive access to the connection.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self.conn.lock();
        f(&conn)
    }

    /// Runs `f` inside a transaction; commits on `Ok`, rolls back on `Err`.
    pub fn tx<T>(&self, f: impl FnOnce(&Transaction) -> AppResult<T>) -> AppResult<T> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // WAL is not available for in-memory databases; ignore the returned mode.
    let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

pub fn user_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

fn backup(conn: &Connection, path: &Path, version: i64) -> AppResult<()> {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("wisp.db");
    let target = path.with_file_name(format!("{file_name}.v{version}.bak"));
    if target.exists() {
        std::fs::remove_file(&target)?;
    }
    let target_str = target.to_string_lossy().to_string();
    conn.execute("VACUUM INTO ?1", [target_str])?;
    log::info!("database backup written before migrating from v{version}");
    Ok(())
}

/// Applies all pending migrations in order. Each step is atomic.
pub fn migrate(conn: &Connection) -> AppResult<()> {
    let current = user_version(conn)?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let next = index as i64 + 1;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = conn.execute_batch(sql).and_then(|_| conn.pragma_update(None, "user_version", next));
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(AppError::from(e));
            }
        }
        log::info!("database migrated to v{next}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_db_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wisp-dock-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("wisp.db")
    }

    #[test]
    fn fresh_database_reaches_latest_version() {
        let db = Db::open_in_memory().unwrap();
        let v = db.with(|c| user_version(c)).unwrap();
        assert_eq!(v, LATEST_VERSION);
    }

    #[test]
    fn migration_from_v1_preserves_data_and_writes_backup() {
        let path = temp_db_path("migrate");
        {
            // Build a representative v1 database by applying only the first migration.
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute("INSERT INTO settings (key, value) VALUES ('app', '{\"theme\":\"light\"}')", []).unwrap();
            conn.execute(
                "INSERT INTO clipboard_items (content, content_hash, created_at, last_used_at, pinned, size_bytes)
                 VALUES ('hello', 'h1', 1, 1, 1, 5)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO snippets (title, content, created_at, updated_at) VALUES ('t', 'c', 1, 1)",
                [],
            )
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        db.with(|c| {
            assert_eq!(user_version(c)?, LATEST_VERSION);
            let theme: String = c.query_row("SELECT value FROM settings WHERE key='app'", [], |r| r.get(0))?;
            assert!(theme.contains("light"));
            let pinned: i64 = c.query_row("SELECT pinned FROM clipboard_items WHERE content='hello'", [], |r| r.get(0))?;
            assert_eq!(pinned, 1);
            let snippets: i64 = c.query_row("SELECT COUNT(*) FROM snippets", [], |r| r.get(0))?;
            assert_eq!(snippets, 1);
            // New tables exist and are usable.
            c.execute("INSERT INTO tasks (title, created_at, updated_at) VALUES ('x', 1, 1)", [])?;
            Ok(())
        })
        .unwrap();
        let backup = path.with_file_name("wisp.db.v1.bak");
        assert!(backup.exists(), "backup should be written before migrating");
        let b = Connection::open(&backup).unwrap();
        assert_eq!(user_version(&b).unwrap(), 1);
    }

    #[test]
    fn newer_database_is_refused_without_modification() {
        let path = temp_db_path("newer");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", LATEST_VERSION + 5).unwrap();
        }
        let err = Db::open(&path).err().expect("should refuse");
        assert_eq!(err.code, "storage_newer");
        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION + 5);
    }

    #[test]
    fn reopening_is_idempotent() {
        let path = temp_db_path("reopen");
        drop(Db::open(&path).unwrap());
        drop(Db::open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
        assert!(!path.with_file_name("wisp.db.v2.bak").exists());
    }
}
