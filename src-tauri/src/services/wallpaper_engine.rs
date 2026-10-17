//! Wallpaper Engine (Steam) integration through its official command-line control
//! (`wallpaper64.exe -control …`, see help.wallpaperengine.io → "Command line").
//!
//! The install is found from Steam's library folders. Wallpapers are read from the Workshop
//! folder (`steamapps/workshop/content/431960/*`) and the app's own `projects/myprojects` and
//! `projects/defaultprojects`. The UI only ever passes back ids from the last scan; paths
//! handed to Wallpaper Engine always come from that scan, never from the UI.

use crate::error::{AppError, AppResult};
use crate::util::data_url;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const WORKSHOP_APP_ID: &str = "431960";
const MAX_GIF_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeWallpaper {
    pub id: String,
    pub title: String,
    /// "video", "scene", "web", "application"…
    pub kind: String,
    /// "workshop", "mine" or "builtin"
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeStatus {
    pub installed: bool,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum WeControl {
    Play,
    Pause,
    Mute,
    Unmute,
}

/// id → wallpaper folder, from the most recent scan.
static SCANNED: Mutex<Option<HashMap<String, PathBuf>>> = Mutex::new(None);

/// Steam library roots (each containing `steamapps`).
fn steam_libraries() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut steam_dirs = vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"C:\Program Files\Steam")];
    if let Some(p) = registry_steam_path() {
        steam_dirs.insert(0, p);
    }
    for steam in steam_dirs {
        if !steam.join("steamapps").is_dir() {
            continue;
        }
        roots.push(steam.clone());
        // Additional libraries are listed in libraryfolders.vdf as  "path"  "D:\\SteamLibrary"
        if let Ok(vdf) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
            for line in vdf.lines() {
                let parts: Vec<&str> = line.split('"').collect();
                if parts.len() >= 4 && parts[1] == "path" {
                    roots.push(PathBuf::from(parts[3].replace("\\\\", "\\")));
                }
            }
        }
    }
    let mut seen = Vec::new();
    roots.retain(|r| {
        let key = r.to_string_lossy().to_lowercase();
        if seen.contains(&key) { false } else { seen.push(key); true }
    });
    roots
}

fn registry_steam_path() -> Option<PathBuf> {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Valve\Steam"),
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    }
    .is_ok();
    if !ok {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..len]).replace('/', "\\")))
}

/// (install dir, wallpaper64.exe) if Wallpaper Engine is installed.
fn install() -> Option<(PathBuf, PathBuf, PathBuf)> {
    for lib in steam_libraries() {
        let dir = lib.join("steamapps").join("common").join("wallpaper_engine");
        let exe = dir.join("wallpaper64.exe");
        if exe.is_file() {
            return Some((dir, exe, lib.join("steamapps").join("workshop").join("content").join(WORKSHOP_APP_ID)));
        }
    }
    None
}

fn is_running() -> bool {
    std::process::Command::new(std::env::var("SystemRoot").map(|r| format!(r"{r}\System32\tasklist.exe")).unwrap_or_else(|_| "tasklist".into()))
            .args(["/FI", "IMAGENAME eq wallpaper64.exe", "/NH"])
            .creation_flags_hidden()
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("wallpaper64.exe"))
            .unwrap_or(false)
}

/// Runs child processes without flashing a console window.
trait HiddenWindow {
    fn creation_flags_hidden(&mut self) -> &mut Self;
}
impl HiddenWindow for std::process::Command {
    fn creation_flags_hidden(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        self.creation_flags(CREATE_NO_WINDOW)
    }
}

pub fn status() -> WeStatus {
    match install() {
        Some(_) => WeStatus { installed: true, running: is_running() },
        None => WeStatus { installed: false, running: false },
    }
}

fn read_project(dir: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(dir.join("project.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("Untitled").trim().to_string();
    let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("").to_lowercase();
    Some((title, kind))
}

/// All wallpapers Wallpaper Engine knows about, newest Workshop subscriptions first.
pub fn list() -> AppResult<Vec<WeWallpaper>> {
    let (dir, _, workshop) = install().ok_or_else(not_installed)?;
    let sources: [(&'static str, PathBuf); 3] = [
        ("workshop", workshop),
        ("mine", dir.join("projects").join("myprojects")),
        ("builtin", dir.join("projects").join("defaultprojects")),
    ];
    let mut out = Vec::new();
    let mut map = HashMap::new();
    for (source, root) in sources {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        let mut entries: Vec<_> = rd.flatten().filter(|e| e.path().is_dir()).collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
        for e in entries {
            let path = e.path();
            let Some((title, kind)) = read_project(&path) else { continue };
            let id = format!("{source}:{}", e.file_name().to_string_lossy());
            map.insert(id.clone(), path);
            out.push(WeWallpaper { id, title, kind, source });
        }
    }
    *SCANNED.lock() = Some(map);
    Ok(out)
}

fn folder(id: &str) -> AppResult<PathBuf> {
    SCANNED
        .lock()
        .as_ref()
        .and_then(|m| m.get(id).cloned())
        .ok_or_else(|| AppError::not_found("That Wallpaper Engine wallpaper"))
}

/// Preview image as a data URL: GIFs as-is (animated) when small enough, stills downscaled.
pub fn preview(id: &str) -> AppResult<Option<String>> {
    let dir = folder(id)?;
    let text = std::fs::read_to_string(dir.join("project.json"))?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let Some(name) = v.get("preview").and_then(|p| p.as_str()) else { return Ok(None) };
    // Only a plain file name inside the wallpaper folder.
    if name.contains(['/', '\\']) || name.contains("..") {
        return Ok(None);
    }
    let path = dir.join(name);
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let Ok(meta) = std::fs::metadata(&path) else { return Ok(None) };
    if ext == "gif" {
        if meta.len() > MAX_GIF_BYTES {
            return Ok(None);
        }
        return Ok(Some(data_url("image/gif", &std::fs::read(&path)?)));
    }
    let img = match image::open(&path) {
        Ok(i) => i,
        Err(_) => return Ok(None),
    };
    let img = img.resize(640, 640, image::imageops::FilterType::Triangle);
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img.to_rgb8())
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82))
        .map_err(|e| AppError::internal(e.to_string()))?;
    Ok(Some(data_url("image/jpeg", &out.into_inner())))
}

fn run(args: &[&std::ffi::OsStr]) -> AppResult<()> {
    let (_, exe, _) = install().ok_or_else(not_installed)?;
    std::process::Command::new(exe)
        .args(args)
        .creation_flags_hidden()
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::native("Wallpaper Engine could not be started.", e.to_string()))
}

/// Applies a wallpaper (starts Wallpaper Engine if needed).
pub fn apply(id: &str) -> AppResult<()> {
    let project = folder(id)?.join("project.json");
    run(&["-control".as_ref(), "openWallpaper".as_ref(), "-file".as_ref(), project.as_os_str()])
}

pub fn control(c: WeControl) -> AppResult<()> {
    let cmd = match c {
        WeControl::Play => "play",
        WeControl::Pause => "pause",
        WeControl::Mute => "mute",
        WeControl::Unmute => "unmute",
    };
    run(&["-control".as_ref(), cmd.as_ref()])
}

fn not_installed() -> AppError {
    AppError::new("we_missing", "Wallpaper Engine isn't installed (it's looked for in your Steam libraries).")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ids_are_rejected() {
        assert!(folder("workshop:does-not-exist").is_err());
        assert!(apply("../../evil").is_err());
    }

    /// Runs against a real install when one exists (skips otherwise).
    #[test]
    fn scans_a_real_install_when_present() {
        if !status().installed {
            return;
        }
        let all = list().unwrap();
        assert!(!all.is_empty(), "an install should have at least the built-in wallpapers");
        let with_preview = all.iter().take(10).filter(|w| preview(&w.id).ok().flatten().is_some()).count();
        println!("wallpaper engine: {} wallpapers, {with_preview}/10 previews", all.len());
        assert!(with_preview > 0);
    }

    #[test]
    fn controls_parse() {
        assert_eq!(serde_json::from_str::<WeControl>("\"pause\"").unwrap(), WeControl::Pause);
        assert!(serde_json::from_str::<WeControl>("\"openWallpaper\"").is_err());
    }
}
