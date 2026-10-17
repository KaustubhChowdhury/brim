//! Local wallpaper gallery.
//!
//! Imported images are *copied* into the app's data folder (`wallpapers/`), named by
//! content hash, so the gallery keeps working if the original is moved and duplicate
//! imports are detected. A JPEG thumbnail is generated into `thumbnails/`. Removing a
//! wallpaper deletes both files. If the copied file goes missing it is flagged in the
//! gallery and can be removed.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::util::{data_url, now_ms, sha256_hex};
use image::imageops::FilterType;
use image::{DynamicImage, ImageReader};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub const ALLOWED_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "bmp"];
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
const THUMB_W: u32 = 320;
const THUMB_H: u32 = 200;
const PREVIEW_MAX: u32 = 1280;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wallpaper {
    pub id: i64,
    pub title: String,
    pub favourite: bool,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub created_at: i64,
    pub missing: bool,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: usize,
    pub duplicates: usize,
    pub failed: Vec<String>,
}

pub struct Dirs {
    pub images: PathBuf,
    pub thumbs: PathBuf,
}

fn decode(path: &Path) -> AppResult<DynamicImage> {
    ImageReader::open(path)?
        .with_guessed_format()?
        .decode()
        .map_err(|e| AppError::invalid("This file is not a supported image.").with_detail(e.to_string()))
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> AppResult<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    rgb.write_with_encoder(encoder).map_err(|e| AppError::internal(e.to_string()))?;
    Ok(out.into_inner())
}

/// Imports one file. Returns `Ok(false)` when it was already in the gallery.
pub fn import_one(db: &Db, dirs: &Dirs, source: &Path) -> AppResult<bool> {
    let ext = source.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AppError::invalid("Only JPG, PNG and BMP images can be used as wallpapers."));
    }
    let meta = std::fs::metadata(source).map_err(|_| AppError::new("missing_path", "That file could not be found."))?;
    if !meta.is_file() {
        return Err(AppError::invalid("Choose an image file."));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(AppError::invalid("Images must be 50 MB or smaller."));
    }
    let bytes = std::fs::read(source)?;
    let hash = sha256_hex(&bytes)[..32].to_string();
    std::fs::create_dir_all(&dirs.images)?;
    std::fs::create_dir_all(&dirs.thumbs)?;
    let dest = dirs.images.join(format!("{hash}.{ext}"));
    let dest_str = dest.to_string_lossy().to_string();
    let exists: Option<i64> =
        db.with(|c| Ok(c.query_row("SELECT id FROM wallpapers WHERE path = ?1", [&dest_str], |r| r.get(0)).optional()?))?;
    if exists.is_some() && dest.exists() {
        return Ok(false);
    }

    let img = decode(source)?;
    let (w, h) = (img.width(), img.height());
    let thumb = img.resize(THUMB_W, THUMB_H, FilterType::Triangle);
    let thumb_path = dirs.thumbs.join(format!("{hash}.jpg"));
    std::fs::write(&thumb_path, encode_jpeg(&thumb, 82)?)?;
    std::fs::write(&dest, &bytes)?;

    let title = source
        .file_stem()
        .map(|s| crate::util::truncate_chars(&s.to_string_lossy(), 80).to_string())
        .unwrap_or_else(|| "Wallpaper".into());
    db.with(|c| {
        c.execute(
            "INSERT INTO wallpapers (path, title, thumbnail_path, width, height, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET thumbnail_path = excluded.thumbnail_path",
            params![dest_str, title, thumb_path.to_string_lossy(), w as i64, h as i64, now_ms()],
        )?;
        Ok(())
    })?;
    Ok(true)
}

pub fn import(db: &Db, dirs: &Dirs, sources: &[PathBuf]) -> ImportResult {
    let mut result = ImportResult { imported: 0, duplicates: 0, failed: vec![] };
    for src in sources {
        match import_one(db, dirs, src) {
            Ok(true) => result.imported += 1,
            Ok(false) => result.duplicates += 1,
            Err(e) => {
                let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                result.failed.push(format!("{name}: {}", e.message));
            }
        }
    }
    result
}

pub fn list(db: &Db) -> AppResult<Vec<Wallpaper>> {
    let rows: Vec<(i64, String, String, bool, Option<i64>, Option<i64>, i64, Option<String>)> = db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT id, path, title, favourite, width, height, created_at, thumbnail_path FROM wallpapers
             ORDER BY favourite DESC, created_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    Ok(rows
        .into_iter()
        .map(|(id, path, title, favourite, width, height, created_at, thumb)| {
            let thumbnail = thumb.and_then(|t| std::fs::read(t).ok()).map(|b| data_url("image/jpeg", &b));
            Wallpaper { id, title, favourite, width, height, created_at, missing: !Path::new(&path).is_file(), thumbnail }
        })
        .collect())
}

pub fn path_of(db: &Db, id: i64) -> AppResult<PathBuf> {
    let path: Option<String> =
        db.with(|c| Ok(c.query_row("SELECT path FROM wallpapers WHERE id = ?1", [id], |r| r.get(0)).optional()?))?;
    let path = PathBuf::from(path.ok_or_else(|| AppError::not_found("That wallpaper"))?);
    if !path.is_file() {
        return Err(AppError::new("missing_path", "This wallpaper's image file is missing. Remove it and import it again."));
    }
    Ok(path)
}

/// A larger JPEG preview (max 1280 px) as a data URL.
pub fn preview(db: &Db, id: i64) -> AppResult<String> {
    let img = decode(&path_of(db, id)?)?;
    let scaled = if img.width() > PREVIEW_MAX || img.height() > PREVIEW_MAX {
        img.resize(PREVIEW_MAX, PREVIEW_MAX, FilterType::Triangle)
    } else {
        img
    };
    Ok(data_url("image/jpeg", &encode_jpeg(&scaled, 85)?))
}

pub fn set_favourite(db: &Db, id: i64, favourite: bool) -> AppResult<()> {
    let n = db.with(|c| Ok(c.execute("UPDATE wallpapers SET favourite = ?2 WHERE id = ?1", params![id, favourite as i64])?))?;
    if n == 0 {
        return Err(AppError::not_found("That wallpaper"));
    }
    Ok(())
}

pub fn rename(db: &Db, id: i64, title: &str) -> AppResult<()> {
    let title = crate::util::clean_text(title, "Name", 1, 80)?;
    let n = db.with(|c| Ok(c.execute("UPDATE wallpapers SET title = ?2 WHERE id = ?1", params![id, title])?))?;
    if n == 0 {
        return Err(AppError::not_found("That wallpaper"));
    }
    Ok(())
}

/// Removes the gallery entry and its copied image and thumbnail.
pub fn remove(db: &Db, id: i64) -> AppResult<()> {
    let row: Option<(String, Option<String>)> = db.with(|c| {
        Ok(c.query_row("SELECT path, thumbnail_path FROM wallpapers WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?)
    })?;
    let Some((path, thumb)) = row else { return Ok(()) };
    db.with(|c| {
        c.execute("DELETE FROM wallpapers WHERE id = ?1", [id])?;
        Ok(())
    })?;
    let _ = std::fs::remove_file(path);
    if let Some(t) = thumb {
        let _ = std::fs::remove_file(t);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(name: &str) -> (PathBuf, Dirs) {
        let root = std::env::temp_dir().join(format!("wisp-wall-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        (root.clone(), Dirs { images: root.join("wallpapers"), thumbs: root.join("thumbnails") })
    }

    fn write_png(path: &Path, w: u32, h: u32) {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 255) as u8, (y % 255) as u8, 128]));
        img.save(path).unwrap();
    }

    #[test]
    fn import_dedup_preview_and_remove() {
        let (root, d) = dirs("ok");
        let src = root.join("Sunset.png");
        write_png(&src, 800, 450);
        let db = Db::open_in_memory().unwrap();
        let r = import(&db, &d, &[src.clone(), src.clone()]);
        assert_eq!((r.imported, r.duplicates, r.failed.len()), (1, 1, 0));
        let list1 = list(&db).unwrap();
        assert_eq!(list1.len(), 1);
        let w = &list1[0];
        assert_eq!((w.title.as_str(), w.width, w.height, w.missing), ("Sunset", Some(800), Some(450), false));
        assert!(w.thumbnail.as_deref().unwrap().starts_with("data:image/jpeg;base64,"));
        // The gallery uses the app's copy, so deleting the original does not break it.
        std::fs::remove_file(&src).unwrap();
        assert!(path_of(&db, w.id).is_ok());
        assert!(preview(&db, w.id).unwrap().starts_with("data:image/jpeg"));
        set_favourite(&db, w.id, true).unwrap();
        rename(&db, w.id, "Evening").unwrap();
        assert_eq!(list(&db).unwrap()[0].title, "Evening");
        remove(&db, w.id).unwrap();
        assert!(list(&db).unwrap().is_empty());
        assert_eq!(std::fs::read_dir(&d.images).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&d.thumbs).unwrap().count(), 0);
    }

    #[test]
    fn rejects_bad_files_and_flags_missing() {
        let (root, d) = dirs("bad");
        let db = Db::open_in_memory().unwrap();
        let txt = root.join("notes.txt");
        std::fs::write(&txt, "hi").unwrap();
        let fake = root.join("fake.jpg");
        std::fs::write(&fake, "not an image").unwrap();
        let r = import(&db, &d, &[txt, fake, root.join("absent.png")]);
        assert_eq!((r.imported, r.failed.len()), (0, 3));

        let good = root.join("good.png");
        write_png(&good, 64, 64);
        import(&db, &d, &[good]);
        let w = list(&db).unwrap().remove(0);
        for entry in std::fs::read_dir(&d.images).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        assert!(list(&db).unwrap()[0].missing);
        assert_eq!(path_of(&db, w.id).unwrap_err().code, "missing_path");
    }
}
