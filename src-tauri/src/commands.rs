//! Tauri commands: the typed contract between the React UI and the Rust core.
//!
//! Every command is thin: it validates input, calls a service and emits change events.
//! Bodies run inside `guard`, so a panic becomes a structured error instead of a crash,
//! and all of them run off the main thread (`async`) so the UI never stalls.

use crate::dock::{DockMsg, DockState};
use crate::error::{AppError, AppResult};
use crate::services::{
    apps, clipboard as clip, media, productivity as prod, slots, snippets, system, volume, wallpaper_engine, wallpapers,
};
use crate::settings::{CaptureState, Settings, Utility, WallpaperFit};
use crate::state::AppState;
use crate::util::{guard, now_ms};
use crate::win::shell;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt;

type S<'a> = State<'a, Arc<AppState>>;

// ---------------- Dock ----------------

#[tauri::command(async)]
pub fn dock_ready(state: S<'_>) -> AppResult<()> {
    guard("dock_ready", || {
        state.dock.send(DockMsg::Ready);
        Ok(())
    })
}

#[tauri::command(async)]
pub fn dock_get_state(state: S<'_>) -> AppResult<DockState> {
    guard("dock_get_state", || state.dock.state())
}

#[tauri::command(async)]
pub fn dock_expand(state: S<'_>, utility: Option<Utility>) -> AppResult<DockState> {
    guard("dock_expand", || state.dock.expand(utility))
}

/// Plays the startup intro (the UI animates; this makes room for it).
#[tauri::command(async)]
pub fn dock_intro(state: S<'_>) -> AppResult<()> {
    guard("dock_intro", || {
        state.dock.send(DockMsg::Intro);
        Ok(())
    })
}

/// Hovering the collapsed dock grows the island back (`show = true`); `false` collapses it again.
#[tauri::command(async)]
pub fn dock_peek(state: S<'_>, show: bool) -> AppResult<()> {
    guard("dock_peek", || {
        state.dock.send(DockMsg::Peek(show));
        Ok(())
    })
}

#[tauri::command(async)]
pub fn dock_activity_done(state: S<'_>, to_idle: bool) -> AppResult<()> {
    guard("dock_activity_done", || {
        state.dock.send(DockMsg::ActivityDone { to_idle });
        Ok(())
    })
}

#[tauri::command(async)]
pub fn dock_collapse(state: S<'_>, restore_focus: bool) -> AppResult<()> {
    guard("dock_collapse", || state.dock.collapse(restore_focus))
}

pub fn open_settings_window(app: &AppHandle, section: Option<&str>) -> AppResult<()> {
    let section = section.filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).unwrap_or("general");
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.unminimize();
        win.show()?;
        win.set_focus()?;
        let _ = tauri::Emitter::emit_to(app, "settings", "settings:navigate", section);
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App(format!("index.html#/settings/{section}").into()))
        .title("Brim Settings")
        .inner_size(860.0, 640.0)
        .min_inner_size(680.0, 500.0)
        .center()
        .focused(true)
        .build()?;
    Ok(())
}

#[tauri::command(async)]
pub fn dock_open_settings(app: AppHandle, state: S<'_>, section: Option<String>) -> AppResult<()> {
    guard("dock_open_settings", || {
        state.dock.send(DockMsg::Collapse { restore_focus: false, reply: None });
        open_settings_window(&app, section.as_deref())
    })
}

#[tauri::command(async)]
pub fn app_quit(app: AppHandle) -> AppResult<()> {
    crate::shutdown(&app);
    app.exit(0);
    Ok(())
}

// ---------------- Settings / privacy / storage ----------------

#[tauri::command(async)]
pub fn settings_get(state: S<'_>) -> AppResult<Settings> {
    guard("settings_get", || Ok(state.settings()))
}

#[tauri::command(async)]
pub fn settings_update(state: S<'_>, patch: serde_json::Value) -> AppResult<Settings> {
    guard("settings_update", || state.update_settings(&patch))
}

#[tauri::command(async)]
pub fn startup_get(app: AppHandle) -> AppResult<bool> {
    guard("startup_get", || {
        app.autolaunch()
            .is_enabled()
            .map_err(|e| AppError::native("Could not read the startup setting.", e.to_string()))
    })
}

#[tauri::command(async)]
pub fn startup_set(app: AppHandle, enabled: bool) -> AppResult<bool> {
    guard("startup_set", || {
        let al = app.autolaunch();
        let r = if enabled { al.enable() } else { al.disable() };
        r.map_err(|e| AppError::native("Windows did not accept the startup change.", e.to_string()))?;
        al.is_enabled().map_err(|e| AppError::native("Could not read the startup setting.", e.to_string()))
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    data_dir: String,
    database_bytes: u64,
    wallpaper_bytes: u64,
    cache_bytes: u64,
    clipboard: clip::ClipStats,
}

fn dir_size(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path)
        .map(|rd| {
            rd.flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => dir_size(&e.path()),
                    Ok(m) => m.len(),
                    Err(_) => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

#[tauri::command(async)]
pub fn storage_info(state: S<'_>) -> AppResult<StorageInfo> {
    guard("storage_info", || {
        let p = &state.paths;
        let db_bytes = ["", "-wal", "-shm"]
            .iter()
            .filter_map(|suffix| std::fs::metadata(format!("{}{}", p.db_file.display(), suffix)).ok())
            .map(|m| m.len())
            .sum();
        Ok(StorageInfo {
            data_dir: p.data_dir.to_string_lossy().to_string(),
            database_bytes: db_bytes,
            wallpaper_bytes: dir_size(&p.wallpapers) + dir_size(&p.thumbnails),
            cache_bytes: dir_size(&p.icons),
            clipboard: clip::stats(&state.db)?,
        })
    })
}

#[tauri::command(async)]
pub fn storage_open_folder(state: S<'_>) -> AppResult<()> {
    guard("storage_open_folder", || {
        // Only ever opens the app's own data folder.
        shell::shell_open(&state.paths.data_dir.to_string_lossy(), None)
    })
}

/// Deletes every piece of locally stored personal data and returns to first-run state.
#[tauri::command(async)]
pub fn data_delete_all(app: AppHandle, state: S<'_>) -> AppResult<()> {
    guard("data_delete_all", || {
        let st = state.inner().clone();
        // Stop capture first so nothing is written while we clear.
        // Keep the dock reachable with the same shortcut.
        let current = st.settings();
        let fresh = Settings { shortcut: current.shortcut, shortcut_customized: current.shortcut_customized, ..Settings::default() };
        st.replace_settings(Settings { clipboard_capture: CaptureState::Off, ..fresh.clone() })?;
        st.db.tx(|tx| {
            for table in [
                "clipboard_items", "app_activity", "applications", "dock_slots", "snippets", "tasks", "reminders",
                "calendar_events", "wallpapers", "settings",
            ] {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
            Ok(())
        })?;
        st.db.with(|c| {
            let _ = c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;");
            Ok(())
        })?;
        // Remove backups made before migrations; they contain personal data too.
        if let Ok(rd) = std::fs::read_dir(&st.paths.data_dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("wisp.db.v") && name.ends_with(".bak") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        for dir in [&st.paths.wallpapers, &st.paths.thumbnails, &st.paths.icons] {
            let _ = std::fs::remove_dir_all(dir);
        }
        st.icon_cache.write().clear();
        *st.last_notice.write() = None;
        let _ = app.autolaunch().disable();
        st.replace_settings(fresh)?;
        st.emit("data:reset", ());
        crate::state::scan_apps(st.clone());
        log::info!("all local data deleted by user");
        Ok(())
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingChoices {
    clipboard_capture: bool,
    launch_at_startup: bool,
}

#[tauri::command(async)]
pub fn onboarding_finish(app: AppHandle, state: S<'_>, choices: OnboardingChoices) -> AppResult<Settings> {
    guard("onboarding_finish", || {
        let al = app.autolaunch();
        let r = if choices.launch_at_startup { al.enable() } else { al.disable() };
        if let Err(e) = r {
            log::warn!("startup preference not applied: {e}");
        }
        state.update_settings(&serde_json::json!({
            "onboardingCompleted": true,
            "clipboardCapture": if choices.clipboard_capture { "on" } else { "off" },
            "clipboardPausedUntil": null,
        }))
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationStatus {
    /// `enabled`, `disabledForApp`, `disabledForUser`, `disabledByPolicy` or `unknown`.
    state: &'static str,
}

#[tauri::command(async)]
pub fn notification_status(app: AppHandle) -> AppResult<NotificationStatus> {
    guard("notification_status", || {
        use windows::UI::Notifications::{NotificationSetting, ToastNotificationManager};
        // Matches the app id the notification plugin uses (identifier when installed).
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let dev = exe_dir
            .map(|d| d.ends_with("target\\debug") || d.ends_with("target\\release"))
            .unwrap_or(false);
        let aumid = if dev {
            r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe".to_string()
        } else {
            app.config().identifier.clone()
        };
        let setting = ToastNotificationManager::CreateToastNotifierWithId(&windows::core::HSTRING::from(aumid))
            .and_then(|n| n.Setting());
        let state = match setting {
            Ok(NotificationSetting::Enabled) => "enabled",
            Ok(NotificationSetting::DisabledForApplication) => "disabledForApp",
            Ok(NotificationSetting::DisabledForUser) => "disabledForUser",
            Ok(NotificationSetting::DisabledByGroupPolicy) => "disabledByPolicy",
            _ => "unknown",
        };
        Ok(NotificationStatus { state })
    })
}

#[tauri::command(async)]
pub fn notification_test(app: AppHandle) -> AppResult<()> {
    guard("notification_test", || {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title("Brim")
            .body("Notifications are working. Reminders will appear like this.")
            .show()
            .map_err(|e| AppError::native("Windows did not show the notification.", e.to_string()))
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    shortcut: String,
    registered: bool,
}

#[tauri::command(async)]
pub fn shortcut_status(app: AppHandle, state: S<'_>) -> AppResult<ShortcutStatus> {
    guard("shortcut_status", || {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let shortcut = state.settings().shortcut;
        let registered = app.global_shortcut().is_registered(shortcut.as_str());
        Ok(ShortcutStatus { shortcut, registered })
    })
}

// ---------------- Slots ----------------

#[tauri::command(async)]
pub fn slots_list(state: S<'_>) -> AppResult<Vec<slots::SlotView>> {
    guard("slots_list", || slots::list(&state.db))
}

#[tauri::command(async)]
pub fn slots_set(state: S<'_>, slot: usize, assignment: slots::SlotAssignment) -> AppResult<Vec<slots::SlotView>> {
    guard("slots_set", || {
        slots::set(&state.db, slot, &assignment)?;
        state.emit("slots:changed", ());
        slots::list(&state.db)
    })
}

#[tauri::command(async)]
pub fn slots_move(state: S<'_>, from: usize, to: usize) -> AppResult<Vec<slots::SlotView>> {
    guard("slots_move", || {
        slots::move_slot(&state.db, from, to)?;
        state.emit("slots:changed", ());
        slots::list(&state.db)
    })
}

#[tauri::command(async)]
pub fn slots_reset(state: S<'_>) -> AppResult<Vec<slots::SlotView>> {
    guard("slots_reset", || {
        slots::reset(&state.db)?;
        state.emit("slots:changed", ());
        slots::list(&state.db)
    })
}

// ---------------- Clipboard ----------------

#[tauri::command(async)]
pub fn clipboard_list(
    state: S<'_>,
    query: String,
    filter: clip::ClipFilter,
    limit: usize,
    offset: usize,
) -> AppResult<clip::ClipPage> {
    guard("clipboard_list", || clip::list(&state.db, &query, filter, limit, offset))
}

#[tauri::command(async)]
pub fn clipboard_copy(state: S<'_>, id: i64) -> AppResult<()> {
    guard("clipboard_copy", || {
        let text = clip::content(&state.db, id)?;
        state.write_clipboard(&text)?;
        clip::mark_used(&state.db, id)?;
        state.emit("clipboard:changed", ());
        Ok(())
    })
}

/// Full text of an entry for the expanded view (capped so huge entries stay responsive).
#[tauri::command(async)]
pub fn clipboard_get(state: S<'_>, id: i64) -> AppResult<String> {
    guard("clipboard_get", || {
        let text = clip::content(&state.db, id)?;
        Ok(crate::util::truncate_chars(&text, 20_000).to_string())
    })
}

#[tauri::command(async)]
pub fn clipboard_pin(state: S<'_>, id: i64, pinned: bool) -> AppResult<()> {
    guard("clipboard_pin", || {
        clip::set_pinned(&state.db, id, pinned)?;
        state.emit("clipboard:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn clipboard_delete(state: S<'_>, id: i64) -> AppResult<()> {
    guard("clipboard_delete", || {
        clip::delete(&state.db, id)?;
        state.emit("clipboard:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn clipboard_clear(state: S<'_>, include_pinned: bool) -> AppResult<usize> {
    guard("clipboard_clear", || {
        let n = clip::clear(&state.db, include_pinned)?;
        state.emit("clipboard:changed", ());
        Ok(n)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardStatus {
    capture: CaptureState,
    paused_until: Option<i64>,
    retention_days: u32,
    max_items: u32,
    stats: clip::ClipStats,
    last_notice: Option<crate::state::ClipNotice>,
}

#[tauri::command(async)]
pub fn clipboard_status(state: S<'_>) -> AppResult<ClipboardStatus> {
    guard("clipboard_status", || {
        let s = state.settings();
        Ok(ClipboardStatus {
            capture: s.clipboard_capture,
            paused_until: s.clipboard_paused_until,
            retention_days: s.clipboard_retention_days,
            max_items: s.clipboard_max_items,
            stats: clip::stats(&state.db)?,
            last_notice: state.last_notice.read().clone(),
        })
    })
}

/// Pauses capture indefinitely (`minutes = None`) or for a number of minutes.
#[tauri::command(async)]
pub fn clipboard_pause(state: S<'_>, minutes: Option<u32>) -> AppResult<Settings> {
    guard("clipboard_pause", || {
        if let Some(m) = minutes {
            if m == 0 || m > 24 * 60 {
                return Err(AppError::invalid("Pause for between 1 minute and 24 hours."));
            }
        }
        if state.settings().clipboard_capture == CaptureState::Off {
            return Err(AppError::invalid("Clipboard capture is turned off."));
        }
        let until = minutes.map(|m| now_ms() + m as i64 * 60_000);
        state.update_settings(&serde_json::json!({ "clipboardCapture": "paused", "clipboardPausedUntil": until }))
    })
}

#[tauri::command(async)]
pub fn clipboard_resume(state: S<'_>) -> AppResult<Settings> {
    guard("clipboard_resume", || {
        state.update_settings(&serde_json::json!({ "clipboardCapture": "on", "clipboardPausedUntil": null }))
    })
}

// ---------------- Apps ----------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppView {
    #[serde(flatten)]
    entry: apps::AppEntry,
    icon: Option<String>,
}

#[tauri::command(async)]
pub fn apps_list(state: S<'_>) -> AppResult<Vec<AppView>> {
    guard("apps_list", || {
        Ok(apps::list(&state.db)?
            .into_iter()
            .map(|entry| {
                let icon = entry.icon_key.as_deref().and_then(|k| state.icon_data_url(k));
                AppView { entry, icon }
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn apps_scanning(state: S<'_>) -> AppResult<bool> {
    Ok(state.scanning.load(std::sync::atomic::Ordering::SeqCst))
}

#[tauri::command(async)]
pub fn apps_launch(state: S<'_>, id: String) -> AppResult<()> {
    guard("apps_launch", || {
        let (target, dir) = apps::launch_target(&state.db, &id)?;
        shell::shell_open(&target, dir.as_deref())?;
        apps::record_launch(&state.db, &id, state.settings().track_recent_apps)?;
        state.emit("apps:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn apps_pin(state: S<'_>, id: String, pinned: bool) -> AppResult<()> {
    guard("apps_pin", || {
        apps::set_pinned(&state.db, &id, pinned)?;
        state.emit("apps:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn apps_reorder(state: S<'_>, ids: Vec<String>) -> AppResult<()> {
    guard("apps_reorder", || {
        if ids.len() > 200 {
            return Err(AppError::invalid("Too many apps."));
        }
        apps::reorder_pins(&state.db, &ids)?;
        state.emit("apps:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn apps_exclude(state: S<'_>, id: String, excluded: bool) -> AppResult<()> {
    guard("apps_exclude", || {
        apps::set_excluded(&state.db, &id, excluded)?;
        state.emit("apps:changed", ());
        Ok(())
    })
}

/// Opens a file picker for a program or shortcut and pins it. Returns the new app id.
#[tauri::command(async)]
pub fn apps_add_custom(app: AppHandle, state: S<'_>) -> AppResult<Option<String>> {
    guard("apps_add_custom", || {
        let picked = with_modal(&app, &state, |builder| {
            builder.set_title("Choose an app").add_filter("Programs and shortcuts", &apps::ALLOWED_EXTENSIONS).blocking_pick_file()
        });
        let Some(file) = picked else { return Ok(None) };
        let path = file.into_path().map_err(|_| AppError::invalid("That location can't be used."))?;
        let path = apps::validate_custom_path(&path)?;
        let id = apps::custom_id(&path);
        let key = apps::icon_key_for(&id);
        let (icon_png, display) = {
            let _com = shell::ComGuard::sta();
            (shell::file_icon_png(&path, apps::ICON_SIZE), shell::file_display_name(&path))
        };
        let icon_key = icon_png.and_then(|png| {
            std::fs::create_dir_all(&state.paths.icons).ok()?;
            std::fs::write(state.paths.icons.join(format!("{key}.png")), png).ok().map(|_| key.clone())
        });
        state.icon_cache.write().remove(&key);
        let name = display
            .or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "App".into());
        let name = crate::util::truncate_chars(name.trim_end_matches(".exe"), 80).to_string();
        let id = apps::add_custom(&state.db, &path, &name, icon_key)?;
        state.emit("apps:changed", ());
        Ok(Some(id))
    })
}

#[tauri::command(async)]
pub fn apps_remove(state: S<'_>, id: String) -> AppResult<()> {
    guard("apps_remove", || {
        apps::remove(&state.db, &id)?;
        state.emit("apps:changed", ());
        state.emit("slots:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn apps_clear_recent(state: S<'_>) -> AppResult<()> {
    guard("apps_clear_recent", || {
        apps::clear_recent(&state.db)?;
        state.emit("apps:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn apps_reset(state: S<'_>) -> AppResult<()> {
    guard("apps_reset", || {
        apps::reset(&state.db)?;
        state.emit("apps:changed", ());
        state.emit("slots:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn apps_refresh(state: S<'_>) -> AppResult<()> {
    guard("apps_refresh", || {
        crate::state::scan_apps(state.inner().clone());
        Ok(())
    })
}

// ---------------- Media ----------------

#[tauri::command(async)]
pub fn media_state(state: S<'_>) -> AppResult<media::MediaState> {
    guard("media_state", || Ok(state.media.state()))
}

#[tauri::command(async)]
pub fn media_command(state: S<'_>, command: media::MediaCommand) -> AppResult<()> {
    guard("media_command", || state.media.command(command))
}

#[tauri::command(async)]
pub fn media_seek(state: S<'_>, position_ms: i64) -> AppResult<()> {
    guard("media_seek", || state.media.seek(position_ms))
}

#[tauri::command(async)]
pub fn media_select(state: S<'_>, session_id: Option<String>) -> AppResult<()> {
    guard("media_select", || state.media.select(session_id))
}

// ---------------- Volume ----------------

#[tauri::command(async)]
pub fn volume_state(state: S<'_>) -> AppResult<volume::VolumeState> {
    guard("volume_state", || Ok(state.volume.state()))
}

#[tauri::command(async)]
pub fn volume_set(state: S<'_>, level: f32) -> AppResult<()> {
    guard("volume_set", || state.volume.set_level(level))
}

#[tauri::command(async)]
pub fn volume_mute(state: S<'_>, muted: bool) -> AppResult<()> {
    guard("volume_mute", || state.volume.set_muted(muted))
}

// ---------------- System ----------------

#[tauri::command(async)]
pub fn system_stats() -> AppResult<system::SystemStats> {
    guard("system_stats", || Ok(system::stats()))
}

#[tauri::command(async)]
pub fn power_action(state: S<'_>, action: system::PowerAction) -> AppResult<()> {
    guard("power_action", || {
        // Close the panel first so it isn't left open over the lock or sign-in screen.
        state.dock.send(DockMsg::Collapse { restore_focus: false, reply: None });
        system::power(action)
    })
}

/// The widget hides itself when nothing is playing (if the user chose that).
#[tauri::command(async)]
pub fn widget_set_visible(app: AppHandle, visible: bool) -> AppResult<()> {
    guard("widget_set_visible", || {
        crate::widget::set_visible(&app, visible);
        Ok(())
    })
}

// ---------------- Web search ----------------

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    Youtube,
    Google,
}

/// Percent-encodes a query for a URL query string (RFC 3986 unreserved characters kept).
pub fn encode_query(q: &str) -> String {
    let mut out = String::with_capacity(q.len() * 3);
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn search_url(engine: SearchEngine, query: &str) -> String {
    let q = encode_query(query);
    match engine {
        SearchEngine::Youtube => format!("https://www.youtube.com/results?search_query={q}"),
        SearchEngine::Google => format!("https://www.google.com/search?q={q}"),
    }
}

/// Opens the search results in the default browser. The URL is always built here from a fixed
/// engine and an encoded query, so the UI can never open an arbitrary address.
#[tauri::command(async)]
pub fn web_search(state: S<'_>, engine: SearchEngine, query: String) -> AppResult<()> {
    guard("web_search", || {
        let query = crate::util::clean_text(&query, "Search", 1, 300)?;
        shell::shell_open(&search_url(engine, &query), None)?;
        if state.settings().collapse_after_launch {
            state.dock.send(DockMsg::Collapse { restore_focus: false, reply: None });
        }
        Ok(())
    })
}

// ---------------- Wallpaper Engine ----------------

#[tauri::command(async)]
pub fn we_status() -> AppResult<wallpaper_engine::WeStatus> {
    guard("we_status", || Ok(wallpaper_engine::status()))
}

#[tauri::command(async)]
pub fn we_list() -> AppResult<Vec<wallpaper_engine::WeWallpaper>> {
    guard("we_list", wallpaper_engine::list)
}

#[tauri::command(async)]
pub fn we_preview(id: String) -> AppResult<Option<String>> {
    guard("we_preview", || wallpaper_engine::preview(&id))
}

#[tauri::command(async)]
pub fn we_apply(id: String) -> AppResult<()> {
    guard("we_apply", || wallpaper_engine::apply(&id))
}

#[tauri::command(async)]
pub fn we_control(control: wallpaper_engine::WeControl) -> AppResult<()> {
    guard("we_control", || wallpaper_engine::control(control))
}

// ---------------- Tasks & reminders & calendar ----------------

fn changed(state: &AppState, what: &str) {
    state.emit("productivity:changed", what.to_string());
}

#[tauri::command(async)]
pub fn tasks_list(state: S<'_>) -> AppResult<Vec<prod::Task>> {
    guard("tasks_list", || prod::list_tasks(&state.db))
}

#[tauri::command(async)]
pub fn tasks_create(state: S<'_>, input: prod::TaskInput) -> AppResult<prod::Task> {
    guard("tasks_create", || {
        let t = prod::create_task(&state.db, &input)?;
        changed(&state, "tasks");
        Ok(t)
    })
}

#[tauri::command(async)]
pub fn tasks_update(state: S<'_>, id: i64, patch: prod::TaskPatch) -> AppResult<prod::Task> {
    guard("tasks_update", || {
        let t = prod::update_task(&state.db, id, &patch)?;
        changed(&state, "tasks");
        Ok(t)
    })
}

#[tauri::command(async)]
pub fn tasks_delete(state: S<'_>, id: i64) -> AppResult<()> {
    guard("tasks_delete", || {
        prod::delete_task(&state.db, id)?;
        changed(&state, "tasks");
        Ok(())
    })
}

#[tauri::command(async)]
pub fn tasks_clear_completed(state: S<'_>) -> AppResult<usize> {
    guard("tasks_clear_completed", || {
        let n = prod::clear_completed_tasks(&state.db)?;
        changed(&state, "tasks");
        Ok(n)
    })
}

#[tauri::command(async)]
pub fn reminders_list(state: S<'_>) -> AppResult<Vec<prod::Reminder>> {
    guard("reminders_list", || prod::list_reminders(&state.db))
}

#[tauri::command(async)]
pub fn reminders_create(state: S<'_>, input: prod::ReminderInput) -> AppResult<prod::Reminder> {
    guard("reminders_create", || {
        let r = prod::create_reminder(&state.db, &input, now_ms())?;
        let _ = state.scheduler_wake.send(());
        changed(&state, "reminders");
        Ok(r)
    })
}

#[tauri::command(async)]
pub fn reminders_update(state: S<'_>, id: i64, input: prod::ReminderInput) -> AppResult<prod::Reminder> {
    guard("reminders_update", || {
        let r = prod::update_reminder(&state.db, id, &input, now_ms())?;
        let _ = state.scheduler_wake.send(());
        changed(&state, "reminders");
        Ok(r)
    })
}

#[tauri::command(async)]
pub fn reminders_dismiss(state: S<'_>, id: i64) -> AppResult<prod::Reminder> {
    guard("reminders_dismiss", || {
        let r = prod::dismiss_reminder(&state.db, id, now_ms())?;
        changed(&state, "reminders");
        Ok(r)
    })
}

#[tauri::command(async)]
pub fn reminders_delete(state: S<'_>, id: i64) -> AppResult<()> {
    guard("reminders_delete", || {
        prod::delete_reminder(&state.db, id)?;
        changed(&state, "reminders");
        Ok(())
    })
}

#[tauri::command(async)]
pub fn reminders_clear_dismissed(state: S<'_>) -> AppResult<usize> {
    guard("reminders_clear_dismissed", || {
        let n = prod::clear_dismissed_reminders(&state.db)?;
        changed(&state, "reminders");
        Ok(n)
    })
}

#[tauri::command(async)]
pub fn calendar_list(state: S<'_>, from: i64, to: i64) -> AppResult<Vec<prod::CalendarEvent>> {
    guard("calendar_list", || prod::list_events(&state.db, from, to))
}

#[tauri::command(async)]
pub fn calendar_create(state: S<'_>, input: prod::EventInput) -> AppResult<prod::CalendarEvent> {
    guard("calendar_create", || {
        let e = prod::create_event(&state.db, &input)?;
        changed(&state, "calendar");
        Ok(e)
    })
}

#[tauri::command(async)]
pub fn calendar_update(state: S<'_>, id: i64, input: prod::EventInput) -> AppResult<prod::CalendarEvent> {
    guard("calendar_update", || {
        let e = prod::update_event(&state.db, id, &input)?;
        changed(&state, "calendar");
        Ok(e)
    })
}

#[tauri::command(async)]
pub fn calendar_delete(state: S<'_>, id: i64) -> AppResult<()> {
    guard("calendar_delete", || {
        prod::delete_event(&state.db, id)?;
        changed(&state, "calendar");
        Ok(())
    })
}

// ---------------- Snippets ----------------

#[tauri::command(async)]
pub fn snippets_list(state: S<'_>) -> AppResult<Vec<snippets::Snippet>> {
    guard("snippets_list", || snippets::list(&state.db))
}

#[tauri::command(async)]
pub fn snippets_create(state: S<'_>, input: snippets::SnippetInput) -> AppResult<snippets::Snippet> {
    guard("snippets_create", || {
        let s = snippets::create(&state.db, &input)?;
        state.emit("snippets:changed", ());
        Ok(s)
    })
}

#[tauri::command(async)]
pub fn snippets_update(state: S<'_>, id: i64, input: snippets::SnippetInput) -> AppResult<snippets::Snippet> {
    guard("snippets_update", || {
        let s = snippets::update(&state.db, id, &input)?;
        state.emit("snippets:changed", ());
        Ok(s)
    })
}

#[tauri::command(async)]
pub fn snippets_delete(state: S<'_>, id: i64) -> AppResult<()> {
    guard("snippets_delete", || {
        snippets::delete(&state.db, id)?;
        state.emit("snippets:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn snippets_copy(state: S<'_>, id: i64) -> AppResult<()> {
    guard("snippets_copy", || {
        let s = snippets::get(&state.db, id)?;
        state.write_clipboard(&s.content)?;
        snippets::mark_used(&state.db, id)?;
        state.emit("snippets:changed", ());
        Ok(())
    })
}

// ---------------- Wallpapers ----------------

fn wallpaper_dirs(state: &AppState) -> wallpapers::Dirs {
    wallpapers::Dirs { images: state.paths.wallpapers.clone(), thumbs: state.paths.thumbnails.clone() }
}

/// Shows a native dialog parented to the dock, keeping the dock open while it is up.
fn with_modal<T>(
    app: &AppHandle,
    state: &AppState,
    f: impl FnOnce(tauri_plugin_dialog::FileDialogBuilder<tauri::Wry>) -> T,
) -> T {
    state.dock.send(DockMsg::ModalOpen(true));
    let mut builder = app.dialog().file();
    if let Some(win) = app.get_webview_window("dock") {
        builder = builder.set_parent(&win);
    }
    let result = f(builder);
    state.dock.send(DockMsg::ModalOpen(false));
    if let Some(win) = app.get_webview_window("dock") {
        let _ = win.hwnd().map(crate::win::window::activate);
    }
    result
}

#[tauri::command(async)]
pub fn wallpapers_list(state: S<'_>) -> AppResult<Vec<wallpapers::Wallpaper>> {
    guard("wallpapers_list", || wallpapers::list(&state.db))
}

#[tauri::command(async)]
pub fn wallpapers_import(app: AppHandle, state: S<'_>) -> AppResult<Option<wallpapers::ImportResult>> {
    guard("wallpapers_import", || {
        let picked = with_modal(&app, &state, |b| {
            b.set_title("Import wallpapers").add_filter("Images", &wallpapers::ALLOWED_EXTENSIONS).blocking_pick_files()
        });
        let Some(files) = picked else { return Ok(None) };
        let paths: Vec<std::path::PathBuf> = files.into_iter().filter_map(|f| f.into_path().ok()).take(50).collect();
        let result = wallpapers::import(&state.db, &wallpaper_dirs(&state), &paths);
        state.emit("wallpapers:changed", ());
        Ok(Some(result))
    })
}

#[tauri::command(async)]
pub fn wallpapers_preview(state: S<'_>, id: i64) -> AppResult<String> {
    guard("wallpapers_preview", || wallpapers::preview(&state.db, id))
}

#[tauri::command(async)]
pub fn wallpapers_apply(state: S<'_>, id: i64, fit: Option<WallpaperFit>) -> AppResult<()> {
    guard("wallpapers_apply", || {
        let path = wallpapers::path_of(&state.db, id)?;
        let fit = fit.unwrap_or(state.settings().wallpaper_fit);
        shell::apply_wallpaper(&path, fit)
    })
}

#[tauri::command(async)]
pub fn wallpapers_favourite(state: S<'_>, id: i64, favourite: bool) -> AppResult<()> {
    guard("wallpapers_favourite", || {
        wallpapers::set_favourite(&state.db, id, favourite)?;
        state.emit("wallpapers:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn wallpapers_rename(state: S<'_>, id: i64, title: String) -> AppResult<()> {
    guard("wallpapers_rename", || {
        wallpapers::rename(&state.db, id, &title)?;
        state.emit("wallpapers:changed", ());
        Ok(())
    })
}

#[tauri::command(async)]
pub fn wallpapers_remove(state: S<'_>, id: i64) -> AppResult<()> {
    guard("wallpapers_remove", || {
        wallpapers::remove(&state.db, id)?;
        state.emit("wallpapers:changed", ());
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_urls_are_encoded() {
        assert_eq!(search_url(SearchEngine::Youtube, "how to cook spaghetti"), "https://www.youtube.com/results?search_query=how+to+cook+spaghetti");
        assert_eq!(search_url(SearchEngine::Google, "a&b=c/d"), "https://www.google.com/search?q=a%26b%3Dc%2Fd");
        assert_eq!(encode_query("café"), "caf%C3%A9");
    }
}
