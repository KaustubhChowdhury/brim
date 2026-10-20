// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod dock;
mod error;
mod settings;
mod state;
mod tray;
mod util;
mod widget;

mod services {
    pub mod apps;
    pub mod clipboard;
    pub mod media;
    pub mod productivity;
    pub mod quick;
    pub mod slots;
    pub mod snippets;
    pub mod system;
    pub mod volume;
    pub mod wallpaper_engine;
    pub mod wallpapers;
}

mod win {
    pub mod clipboard;
    pub mod display;
    pub mod os_events;
    pub mod shell;
    pub mod volume_keys;
    pub mod window;
}

use crate::db::Db;
use crate::dock::{ActivityKind, DockMsg};
use crate::services::{clipboard as clip, media::MediaService, volume::VolumeService};
use crate::state::{AppState, CaptureJob, Paths};
use crate::util::now_ms;
use crate::win::os_events::{self, ClipPolicy, OsEvent};
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

/// Releases OS registrations (app bar, clipboard listener, session notifications).
pub fn shutdown(_app: &AppHandle) {
    win::volume_keys::disable();
    os_events::stop();
}

fn fatal(message: &str) -> ! {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    log::error!("fatal: {message}");
    unsafe {
        MessageBoxW(None, &HSTRING::from(message), &HSTRING::from("Brim"), MB_OK | MB_ICONERROR);
    }
    std::process::exit(1);
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = handle.path().app_data_dir()?;
    let cache_dir = handle.path().app_cache_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let paths = Paths {
        db_file: data_dir.join("wisp.db"),
        wallpapers: data_dir.join("wallpapers"),
        thumbnails: data_dir.join("thumbnails"),
        icons: cache_dir.join("icons"),
        data_dir,
    };

    let db = match Db::open(&paths.db_file) {
        Ok(db) => Arc::new(db),
        Err(e) => fatal(&e.message),
    };
    let settings = settings::load(&db)?;
    log::info!("Brim {} starting", handle.package_info().version);
    let shared = Arc::new(RwLock::new(settings));

    let dock_window = app.get_webview_window("dock").ok_or("dock window missing from configuration")?;
    let dock_hwnd = win::window::raw(dock_window.hwnd()?);
    let dock = dock::start(handle.clone(), dock_hwnd, db.clone(), shared.clone());

    // Media: forward state to the UI, and pop the "now playing" activity when a song starts
    // or changes (not for whatever was already playing when the app launched).
    let media_handle = handle.clone();
    let media_dock = dock.clone();
    let last_track: Mutex<Option<Option<(String, bool)>>> = Mutex::new(None);
    let media = MediaService::start(Arc::new(move |s| {
        let _ = media_handle.emit("media:state", s.clone());
        let now = s
            .active
            .as_ref()
            .filter(|a| !a.title.is_empty())
            .map(|a| (format!("{}|{}|{}", a.session_id, a.title, a.artist), a.status == "playing"));
        let mut last = last_track.lock();
        if let (Some(prev), Some((key, true))) = (last.as_ref(), now.as_ref()) {
            let started = match prev {
                Some((prev_key, was_playing)) => prev_key != key || !was_playing,
                None => true,
            };
            if started {
                media_dock.send(DockMsg::Activity(ActivityKind::NowPlaying));
            }
        }
        *last = Some(now);
    }));

    // Volume: forward to the UI; show the volume activity for changes made outside the dock.
    let volume_handle = handle.clone();
    let volume_dock = dock.clone();
    let volume = VolumeService::start(Arc::new(move |v, external| {
        let _ = volume_handle.emit("volume:state", v);
        if external {
            volume_dock.send(DockMsg::Activity(ActivityKind::Volume));
        }
    }));

    let (wake_tx, wake_rx) = channel();
    let state = Arc::new(AppState {
        app: handle.clone(),
        db: db.clone(),
        settings: shared.clone(),
        dock: dock.clone(),
        media,
        volume,
        paths,
        scheduler_wake: wake_tx,
        icon_cache: RwLock::new(Default::default()),
        last_notice: RwLock::new(None),
        scanning: AtomicBool::new(false),
        tray_capture_item: Mutex::new(None),
    });
    app.manage(state.clone());

    // Clipboard capture and OS event wiring. The sink only forwards to channels.
    let capture_tx = state::start_capture_worker(state.clone());
    let sink_state = state.clone();
    let sink_capture = Mutex::new(capture_tx);
    let sink: os_events::Sink = Arc::new(move |event| match event {
        OsEvent::ClipboardText(text) => {
            let _ = sink_capture.lock().send(CaptureJob::Text(text));
        }
        OsEvent::ClipboardSkipped(reason) => {
            let _ = sink_capture.lock().send(CaptureJob::Skipped(reason));
        }
        OsEvent::DisplayChanged => sink_state.dock.send(DockMsg::DisplayChanged),
        OsEvent::Resumed | OsEvent::SessionUnlocked => {
            sink_state.dock.send(DockMsg::DisplayChanged);
            sink_state.media.refresh();
            let _ = sink_state.scheduler_wake.send(());
        }
        OsEvent::FullscreenMaybeChanged | OsEvent::TaskbarCreated => sink_state.dock.send(DockMsg::FullscreenCheck),
    });
    let policy_settings = shared.clone();
    let policy: os_events::PolicyFn = Arc::new(move || {
        let s = policy_settings.read();
        if !s.capture_active(now_ms()) {
            return None;
        }
        Some(ClipPolicy { max_bytes: s.clipboard_max_entry_kb as usize * 1024, ignored_apps: s.clipboard_ignored_apps.clone() })
    });
    os_events::start(sink, policy);

    state::start_scheduler(state.clone(), wake_rx);
    tray::create(&handle, &state)?;

    register_startup_shortcut(&state);
    if let Err(e) = clip::apply_retention(&db, &state.settings(), now_ms()) {
        log::warn!("startup retention failed: {e}");
    }
    state::scan_apps(state.clone());
    migrate_startup_entry(&handle);
    state::apply_volume_keys(&state);
    if let Err(e) = widget::apply(&handle, &db, &state.settings()) {
        log::warn!("music widget unavailable: {e}");
    }

    if !state.settings().onboarding_completed {
        commands::open_settings_window(&handle, Some("welcome"))?;
    }
    Ok(())
}

/// Registers the shortcut. A shortcut the user chose is kept even if it is unavailable (Settings
/// shows a warning). Otherwise the default is used, or the first free fallback if another
/// program already owns it, so the dock is always reachable.
fn register_startup_shortcut(state: &Arc<AppState>) {
    let current = state.settings();
    if current.shortcut_customized {
        if let Err(e) = state::apply_shortcut(&state.app, None, &current.shortcut) {
            log::warn!("global shortcut {} unavailable at startup: {e}", current.shortcut);
        }
        return;
    }
    let candidates = std::iter::once(settings::DEFAULT_SHORTCUT).chain(settings::FALLBACK_SHORTCUTS);
    for candidate in candidates {
        match state::apply_shortcut(&state.app, None, candidate) {
            Ok(()) => {
                if current.shortcut != candidate {
                    let mut s = state.settings.write();
                    s.shortcut = candidate.to_string();
                    if let Err(e) = settings::save(&state.db, &s) {
                        log::warn!("could not save shortcut: {e}");
                    }
                }
                log::info!("global shortcut: {candidate}");
                return;
            }
            Err(_) => log::warn!("shortcut {candidate} is taken by another program"),
        }
    }
    log::error!("no global shortcut could be registered; use the tray icon to open the dock");
}

/// The app was renamed from "Wisp Dock". Move a "launch at sign-in" entry registered under
/// the old name to the new one, so it neither disappears nor starts the app twice.
fn migrate_startup_entry(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    use windows::core::w;
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    const OLD: windows::core::PCWSTR = w!("Wisp Dock");
    let run = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let had_old =
        unsafe { RegGetValueW(HKEY_CURRENT_USER, run, OLD, RRF_RT_REG_SZ, None, None, None) }.is_ok();
    if !had_old {
        return;
    }
    unsafe {
        let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, run, OLD);
        let _ = RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"),
            OLD,
        );
    }
    match app.autolaunch().enable() {
        Ok(()) => log::info!("moved launch-at-sign-in entry to the new app name"),
        Err(e) => log::warn!("could not re-register launch at sign-in: {e}"),
    }
}

fn main() {
    // If this is a second launch, let the running instance take the foreground.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));

    let app = tauri::Builder::default()
        // Must be first so a second instance exits before doing any work (FR-DCK-010).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(state) = app.try_state::<Arc<AppState>>() {
                state.dock.send(DockMsg::Reveal);
            }
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info })
                .max_file_size(1_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        if let Some(state) = app.try_state::<Arc<AppState>>() {
                            state.dock.send(DockMsg::Toggle);
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::Builder::new().args(["--autostart"]).app_name("Brim").build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .on_window_event(|window, event| {
            if window.label() == widget::LABEL {
                if let (WindowEvent::Moved(_), Some(state)) = (event, window.try_state::<Arc<AppState>>()) {
                    widget::moved(window.app_handle(), state.db.clone());
                }
                return;
            }
            if window.label() != "dock" {
                return;
            }
            let Some(state) = window.try_state::<Arc<AppState>>() else { return };
            match event {
                WindowEvent::Focused(focused) => state.dock.send(DockMsg::Focused(*focused)),
                WindowEvent::Moved(_) => state.dock.send(DockMsg::Moved),
                // The dock is never closed by Alt+F4; quit from the tray or settings instead.
                WindowEvent::CloseRequested { api, .. } => api.prevent_close(),
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::dock_ready,
            commands::dock_get_state,
            commands::dock_expand,
            commands::dock_collapse,
            commands::dock_peek,
            commands::dock_intro,
            commands::dock_activity_done,
            commands::volume_state,
            commands::volume_set,
            commands::volume_mute,
            commands::quick_state,
            commands::quick_set,
            commands::quick_set_output,
            commands::quick_open,
            commands::system_stats,
            commands::system_info,
            commands::power_action,
            commands::widget_set_visible,
            commands::web_search,
            commands::we_status,
            commands::we_list,
            commands::we_preview,
            commands::we_apply,
            commands::we_control,
            commands::dock_open_settings,
            commands::app_quit,
            commands::settings_get,
            commands::settings_update,
            commands::startup_get,
            commands::startup_set,
            commands::storage_info,
            commands::storage_open_folder,
            commands::data_delete_all,
            commands::onboarding_finish,
            commands::notification_status,
            commands::notification_test,
            commands::shortcut_status,
            commands::slots_list,
            commands::slots_set,
            commands::slots_move,
            commands::slots_reset,
            commands::clipboard_list,
            commands::clipboard_copy,
            commands::clipboard_get,
            commands::clipboard_pin,
            commands::clipboard_delete,
            commands::clipboard_clear,
            commands::clipboard_status,
            commands::clipboard_pause,
            commands::clipboard_resume,
            commands::apps_list,
            commands::apps_scanning,
            commands::apps_launch,
            commands::apps_pin,
            commands::apps_reorder,
            commands::apps_exclude,
            commands::apps_add_custom,
            commands::apps_remove,
            commands::apps_clear_recent,
            commands::apps_reset,
            commands::apps_refresh,
            commands::media_state,
            commands::media_command,
            commands::media_seek,
            commands::media_select,
            commands::tasks_list,
            commands::tasks_create,
            commands::tasks_update,
            commands::tasks_delete,
            commands::tasks_clear_completed,
            commands::reminders_list,
            commands::reminders_create,
            commands::reminders_update,
            commands::reminders_dismiss,
            commands::reminders_delete,
            commands::reminders_clear_dismissed,
            commands::calendar_list,
            commands::calendar_create,
            commands::calendar_update,
            commands::calendar_delete,
            commands::snippets_list,
            commands::snippets_create,
            commands::snippets_update,
            commands::snippets_delete,
            commands::snippets_copy,
            commands::wallpapers_list,
            commands::wallpapers_import,
            commands::wallpapers_preview,
            commands::wallpapers_apply,
            commands::wallpapers_favourite,
            commands::wallpapers_rename,
            commands::wallpapers_remove,
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| fatal(&format!("Brim could not start: {e}")));

    app.run(|app, event| match event {
        // Closing the settings window must not quit the app; only explicit quits carry a code.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => shutdown(app),
        _ => {}
    });
}
