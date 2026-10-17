//! Shared application state and the background workers that connect services together.

use crate::db::Db;
use crate::dock::{DockHandle, DockMsg};
use crate::error::{AppError, AppResult};
use crate::services::{apps, clipboard as clip, media::MediaService, productivity, volume::VolumeService};
use crate::settings::{self, CaptureState, Settings};
use crate::util::{data_url, now_ms, spawn_supervised};
use crate::win::{os_events, shell};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_notification::NotificationExt;

#[derive(Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub db_file: PathBuf,
    pub wallpapers: PathBuf,
    pub thumbnails: PathBuf,
    pub icons: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipNotice {
    /// `tooLarge`, `secret`, `private` or `ignoredApp`
    pub kind: &'static str,
    pub size_bytes: Option<usize>,
    pub app: Option<String>,
    pub at: i64,
}

pub struct AppState {
    pub app: AppHandle,
    pub db: Arc<Db>,
    pub settings: Arc<RwLock<Settings>>,
    pub dock: DockHandle,
    pub media: MediaService,
    pub volume: VolumeService,
    pub paths: Paths,
    pub scheduler_wake: Sender<()>,
    pub icon_cache: RwLock<HashMap<String, Option<String>>>,
    pub last_notice: RwLock<Option<ClipNotice>>,
    pub scanning: AtomicBool,
    pub tray_capture_item: Mutex<Option<tauri::menu::MenuItem<tauri::Wry>>>,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Err(e) = self.app.emit(event, payload) {
            log::warn!("emit {event} failed: {e}");
        }
    }

    /// Validates and applies a settings patch, then performs side effects.
    pub fn update_settings(&self, patch: &serde_json::Value) -> AppResult<Settings> {
        let mut guard = self.settings.write();
        let old = guard.clone();
        let mut next = old.patched(patch)?;
        if patch.get("shortcut").is_some() {
            next.shortcut_customized = true;
        }
        if next.shortcut != old.shortcut {
            apply_shortcut(&self.app, Some(&old.shortcut), &next.shortcut)?;
        }
        if let Err(e) = settings::save(&self.db, &next) {
            // Roll back the shortcut so runtime state matches what is stored.
            if next.shortcut != old.shortcut {
                let _ = apply_shortcut(&self.app, Some(&next.shortcut), &old.shortcut);
            }
            return Err(e);
        }
        *guard = next.clone();
        drop(guard);
        self.after_settings_change(&old, &next);
        Ok(next)
    }

    /// Replaces settings wholesale (used by "delete all data").
    pub fn replace_settings(&self, next: Settings) -> AppResult<()> {
        let old = self.settings();
        if next.shortcut != old.shortcut {
            apply_shortcut(&self.app, Some(&old.shortcut), &next.shortcut)?;
        }
        settings::save(&self.db, &next)?;
        *self.settings.write() = next.clone();
        self.after_settings_change(&old, &next);
        Ok(())
    }

    fn after_settings_change(&self, old: &Settings, next: &Settings) {
        self.dock.send(DockMsg::SettingsChanged(Box::new(next.clone())));
        if old.handle_volume_keys != next.handle_volume_keys {
            apply_volume_keys_with(&self.app, next.handle_volume_keys);
        }
        if old.music_widget != next.music_widget
            || old.music_widget_style != next.music_widget_style
            || old.music_widget_on_top != next.music_widget_on_top
        {
            if let Err(e) = crate::widget::apply(&self.app, &self.db, next) {
                log::warn!("music widget update failed: {e}");
            }
        }
        if old.clipboard_capture != next.clipboard_capture || old.clipboard_paused_until != next.clipboard_paused_until {
            self.refresh_tray();
            let _ = self.scheduler_wake.send(());
        }
        if old.clipboard_retention_days != next.clipboard_retention_days
            || old.clipboard_max_items != next.clipboard_max_items
        {
            match clip::apply_retention(&self.db, next, now_ms()) {
                Ok(n) if n > 0 => self.emit("clipboard:changed", ()),
                Ok(_) => {}
                Err(e) => log::warn!("retention after settings change failed: {e}"),
            }
        }
        self.emit("settings:changed", next.clone());
    }

    pub fn refresh_tray(&self) {
        let s = self.settings();
        if let Some(item) = self.tray_capture_item.lock().as_ref() {
            let (text, enabled) = match s.clipboard_capture {
                CaptureState::On => ("Pause clipboard capture", true),
                CaptureState::Paused => ("Resume clipboard capture", true),
                CaptureState::Off => ("Clipboard capture is off", false),
            };
            let _ = item.set_text(text);
            let _ = item.set_enabled(enabled);
        }
    }

    pub fn icon_data_url(&self, key: &str) -> Option<String> {
        if let Some(hit) = self.icon_cache.read().get(key) {
            return hit.clone();
        }
        let path = self.paths.icons.join(format!("{key}.png"));
        let value = std::fs::read(path).ok().map(|b| data_url("image/png", &b));
        self.icon_cache.write().insert(key.to_string(), value.clone());
        value
    }

    /// Writes text to the Windows clipboard without capturing it again.
    pub fn write_clipboard(&self, text: &str) -> AppResult<()> {
        let owner = os_events::hidden_window();
        if owner.0.is_null() {
            return Err(AppError::new("clipboard_unavailable", "The clipboard is not available right now."));
        }
        crate::win::clipboard::write_text(owner, text)
    }

    pub fn notice(&self, notice: ClipNotice) {
        *self.last_notice.write() = Some(notice.clone());
        self.emit("clipboard:notice", notice);
    }
}

pub fn apply_shortcut(app: &AppHandle, old: Option<&str>, new: &str) -> AppResult<()> {
    let gs = app.global_shortcut();
    if let Some(old) = old {
        if gs.is_registered(old) {
            let _ = gs.unregister(old);
        }
    }
    if let Err(e) = gs.register(new) {
        // Restore the previous shortcut so the dock stays reachable.
        if let Some(old) = old {
            let _ = gs.register(old);
        }
        return Err(AppError::new(
            "shortcut_unavailable",
            format!("\"{new}\" can't be used. It may be invalid or already taken by another app."),
        )
        .with_detail(e.to_string()));
    }
    Ok(())
}

// ---------- Clipboard capture worker ----------

pub enum CaptureJob {
    Text(String),
    Skipped(os_events::SkipReason),
}

pub fn start_capture_worker(state: Arc<AppState>) -> Sender<CaptureJob> {
    let (tx, rx) = channel::<CaptureJob>();
    let rx = Arc::new(Mutex::new(rx));
    spawn_supervised("clipboard-capture", move || {
        let rx = rx.lock();
        while let Ok(job) = rx.recv() {
            match job {
                CaptureJob::Text(text) => {
                    let s = state.settings();
                    if !s.capture_active(now_ms()) {
                        continue;
                    }
                    match clip::capture(&state.db, &s, &text, now_ms()) {
                        Ok(clip::CaptureOutcome::Inserted(_)) | Ok(clip::CaptureOutcome::Bumped(_)) => {
                            state.emit("clipboard:changed", ())
                        }
                        Ok(clip::CaptureOutcome::TooLarge(n)) => {
                            state.notice(ClipNotice { kind: "tooLarge", size_bytes: Some(n), app: None, at: now_ms() })
                        }
                        Ok(clip::CaptureOutcome::LikelySecret) => {
                            state.notice(ClipNotice { kind: "secret", size_bytes: None, app: None, at: now_ms() })
                        }
                        Ok(clip::CaptureOutcome::Ignored) => {}
                        Err(e) => log::error!("clipboard capture failed: {e}"),
                    }
                }
                CaptureJob::Skipped(reason) => {
                    let notice = match reason {
                        os_events::SkipReason::TooLarge(n) => {
                            ClipNotice { kind: "tooLarge", size_bytes: Some(n), app: None, at: now_ms() }
                        }
                        os_events::SkipReason::MarkedPrivate => {
                            ClipNotice { kind: "private", size_bytes: None, app: None, at: now_ms() }
                        }
                        os_events::SkipReason::IgnoredApp(a) => {
                            ClipNotice { kind: "ignoredApp", size_bytes: None, app: Some(a), at: now_ms() }
                        }
                    };
                    state.notice(notice);
                }
            }
        }
        true // channel closed: stop
    });
    tx
}

// ---------- Reminder scheduler ----------

pub fn start_scheduler(state: Arc<AppState>, wake: Receiver<()>) {
    let wake = Arc::new(Mutex::new(wake));
    spawn_supervised("scheduler", move || {
        let wake = wake.lock();
        loop {
            let now = now_ms();
            expire_timed_pause(&state, now);
            match productivity::claim_due(&state.db, now) {
                Ok(deliveries) if !deliveries.is_empty() => {
                    for d in deliveries {
                        deliver(&state, d);
                    }
                    state.emit("productivity:changed", "reminders");
                }
                Ok(_) => {}
                Err(e) => log::error!("reminder check failed: {e}"),
            }
            let next_reminder = productivity::next_due_at(&state.db).ok().flatten();
            let pause_until = {
                let s = state.settings.read();
                if s.clipboard_capture == CaptureState::Paused { s.clipboard_paused_until } else { None }
            };
            // Wall-clock based: wake at the next deadline, and at least every 30 s so sleep,
            // hibernation or clock changes can never make us miss a reminder for long.
            let next = [next_reminder, pause_until].into_iter().flatten().min();
            let wait_ms = next.map(|t| (t - now_ms()).clamp(0, 30_000)).unwrap_or(30_000) as u64;
            match wake.recv_timeout(Duration::from_millis(wait_ms.max(50))) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return true,
            }
        }
    });
}

fn expire_timed_pause(state: &AppState, now: i64) {
    let expired = {
        let s = state.settings.read();
        s.clipboard_capture == CaptureState::Paused && s.clipboard_paused_until.is_some_and(|t| t <= now)
    };
    if expired {
        let patch = serde_json::json!({ "clipboardCapture": "on", "clipboardPausedUntil": null });
        if let Err(e) = state.update_settings(&patch) {
            log::warn!("could not resume clipboard capture after timed pause: {e}");
        }
    }
}

fn deliver(state: &AppState, delivery: productivity::Delivery) {
    let productivity::Delivery::Notify { reminder, late_by_ms, launch_app } = delivery else {
        return; // missed: shown in the Today view, no notification
    };
    let s = state.settings();
    if s.notifications_enabled {
        let body = if late_by_ms > 120_000 {
            format!("{} (due {} min ago)", reminder.title, late_by_ms / 60_000)
        } else {
            reminder.title.clone()
        };
        if let Err(e) = state.app.notification().builder().title("Reminder").body(body).show() {
            log::warn!("notification failed: {e}");
        }
    }
    if let Some(app_id) = launch_app {
        match apps::launch_target(&state.db, &app_id) {
            Ok((target, dir)) => {
                if let Err(e) = shell::shell_open(&target, dir.as_deref()) {
                    log::warn!("scheduled launch failed: {e}");
                } else {
                    let _ = apps::record_launch(&state.db, &app_id, s.track_recent_apps);
                }
            }
            Err(e) => log::warn!("scheduled launch target unavailable: {e}"),
        }
    }
}

// ---------- App discovery ----------

/// Scans the Start menu on a background STA thread and refreshes the cache.
pub fn scan_apps(state: Arc<AppState>) {
    if state.scanning.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("app-scan".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _com = shell::ComGuard::sta();
                let found = shell::enumerate_apps(apps::ICON_SIZE)?;
                std::fs::create_dir_all(&state.paths.icons)?;
                let discovered: Vec<apps::DiscoveredApp> = found
                    .into_iter()
                    .map(|a| {
                        let id = apps::shell_id(&a.parsing_name);
                        let key = apps::icon_key_for(&id);
                        let icon_key = a.icon_png.and_then(|png| {
                            std::fs::write(state.paths.icons.join(format!("{key}.png")), png).ok().map(|_| key)
                        });
                        apps::DiscoveredApp { name: a.name, parsing_name: a.parsing_name, icon_key }
                    })
                    .collect();
                let count = discovered.len();
                apps::merge_discovered(&state.db, &discovered)?;
                state.icon_cache.write().clear();
                log::info!("app scan found {count} entries");
                Ok::<(), AppError>(())
            }));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log::error!("app scan failed: {e}"),
                Err(_) => log::error!("app scan panicked"),
            }
            state.scanning.store(false, Ordering::SeqCst);
            state.emit("apps:changed", ());
            state.emit("slots:changed", ());
        })
        .expect("spawn app scan");
}

// ---------------- Volume keys ----------------

/// Installs or removes the volume-key hook to match the current setting.
pub fn apply_volume_keys(state: &Arc<AppState>) {
    apply_volume_keys_with(&state.app, state.settings().handle_volume_keys);
}

fn apply_volume_keys_with(app: &AppHandle, enabled: bool) {
    use crate::win::volume_keys::{self, VolumeKey};
    use tauri::Manager;
    if !enabled {
        volume_keys::disable();
        return;
    }
    let Some(state) = app.try_state::<Arc<AppState>>() else { return };
    let (tx, rx) = channel::<VolumeKey>();
    let volume = state.volume.clone();
    let dock = state.dock.clone();
    std::thread::Builder::new()
        .name("volume-key-actions".into())
        .spawn(move || {
            while let Ok(key) = rx.recv() {
                match key {
                    VolumeKey::Up => volume.step(1),
                    VolumeKey::Down => volume.step(-1),
                    VolumeKey::Mute => volume.toggle_mute(),
                }
                dock.send(DockMsg::Activity(crate::dock::ActivityKind::Volume));
            }
        })
        .expect("spawn volume-key-actions thread");
    volume_keys::enable(tx);
}
