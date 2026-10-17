//! System-tray icon and menu (FR-DCK-011).

use crate::dock::DockMsg;
use crate::settings::{CaptureState, TrayClickAction};
use crate::state::AppState;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn create(app: &AppHandle, state: &Arc<AppState>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Brim", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let capture = MenuItem::with_id(app, "capture", "Pause clipboard capture", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Brim", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &capture,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    *state.tray_capture_item.lock() = Some(capture);
    state.refresh_tray();

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Brim")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let Some(state) = app.try_state::<Arc<AppState>>() else { return };
            match event.id().as_ref() {
                "show" => state.dock.send(DockMsg::Reveal),
                "settings" => {
                    if let Err(e) = crate::commands::open_settings_window(app, None) {
                        log::warn!("open settings failed: {e}");
                    }
                }
                "capture" => {
                    let patch = match state.settings().clipboard_capture {
                        CaptureState::On => serde_json::json!({ "clipboardCapture": "paused", "clipboardPausedUntil": null }),
                        CaptureState::Paused => serde_json::json!({ "clipboardCapture": "on", "clipboardPausedUntil": null }),
                        CaptureState::Off => return,
                    };
                    if let Err(e) = state.update_settings(&patch) {
                        log::warn!("tray capture toggle failed: {e}");
                    }
                }
                "quit" => {
                    crate::shutdown(app);
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                let Some(state) = app.try_state::<Arc<AppState>>() else { return };
                match state.settings().tray_click_action {
                    TrayClickAction::Toggle => state.dock.send(DockMsg::Toggle),
                    TrayClickAction::Settings => {
                        let _ = crate::commands::open_settings_window(app, None);
                    }
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
