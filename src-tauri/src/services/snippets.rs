//! Reusable text snippets, stored locally.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::util::{clean_optional, clean_text, now_ms};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

pub const MAX_CONTENT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub category: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetInput {
    pub title: String,
    pub content: String,
    pub category: Option<String>,
}

const COLS: &str = "id, title, content, category, created_at, updated_at, last_used_at";

fn from_row(r: &Row) -> rusqlite::Result<Snippet> {
    Ok(Snippet {
        id: r.get(0)?,
        title: r.get(1)?,
        content: r.get(2)?,
        category: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        last_used_at: r.get(6)?,
    })
}

fn validate(input: &SnippetInput) -> AppResult<(String, String, Option<String>)> {
    let title = clean_text(&input.title, "Snippet name", 1, 80)?;
    if input.content.trim().is_empty() {
        return Err(AppError::invalid("Snippet text is required."));
    }
    if input.content.len() > MAX_CONTENT_BYTES {
        return Err(AppError::invalid("Snippets can be at most 64 KB."));
    }
    let category = clean_optional(input.category.as_deref(), "Category", 40)?;
    Ok((title, input.content.clone(), category))
}

pub fn list(db: &Db) -> AppResult<Vec<Snippet>> {
    db.with(|c| {
        let mut stmt = c.prepare(&format!(
            "SELECT {COLS} FROM snippets ORDER BY COALESCE(last_used_at, updated_at) DESC, id DESC"
        ))?;
        let rows = stmt.query_map([], from_row)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn get(db: &Db, id: i64) -> AppResult<Snippet> {
    db.with(|c| {
        c.query_row(&format!("SELECT {COLS} FROM snippets WHERE id = ?1"), [id], from_row)
            .optional()?
            .ok_or_else(|| AppError::not_found("That snippet"))
    })
}

pub fn create(db: &Db, input: &SnippetInput) -> AppResult<Snippet> {
    let (title, content, category) = validate(input)?;
    let now = now_ms();
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO snippets (title, content, category, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![title, content, category, now],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    get(db, id)
}

pub fn update(db: &Db, id: i64, input: &SnippetInput) -> AppResult<Snippet> {
    let (title, content, category) = validate(input)?;
    let n = db.with(|c| {
        Ok(c.execute(
            "UPDATE snippets SET title = ?2, content = ?3, category = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, title, content, category, now_ms()],
        )?)
    })?;
    if n == 0 {
        return Err(AppError::not_found("That snippet"));
    }
    get(db, id)
}

pub fn mark_used(db: &Db, id: i64) -> AppResult<()> {
    db.with(|c| {
        c.execute("UPDATE snippets SET last_used_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    })
}

pub fn delete(db: &Db, id: i64) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM snippets WHERE id = ?1", [id])?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_and_validation() {
        let db = Db::open_in_memory().unwrap();
        let input = |t: &str, c: &str| SnippetInput { title: t.into(), content: c.into(), category: Some(" shell ".into()) };
        assert!(create(&db, &input("", "x")).is_err());
        assert!(create(&db, &input("t", "   ")).is_err());
        assert!(create(&db, &input("t", &"x".repeat(MAX_CONTENT_BYTES + 1))).is_err());
        let s = create(&db, &input("Deploy", "npm run build\n")).unwrap();
        assert_eq!(s.category.as_deref(), Some("shell"));
        assert_eq!(s.content, "npm run build\n"); // content is preserved verbatim
        let u = update(&db, s.id, &input("Deploy 2", "x")).unwrap();
        assert_eq!(u.title, "Deploy 2");
        mark_used(&db, s.id).unwrap();
        assert!(get(&db, s.id).unwrap().last_used_at.is_some());
        delete(&db, s.id).unwrap();
        assert!(list(&db).unwrap().is_empty());
        assert!(update(&db, s.id, &input("a", "b")).is_err());
    }
}
