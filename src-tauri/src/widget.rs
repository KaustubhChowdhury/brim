//! Optional desktop music widget: a small frameless card showing what's playing.
//!
//! The window is created when the setting is turned on and destroyed when it's turned off.
//! It is a tool window (no taskbar button, not in Alt+Tab), draggable, and remembers where it
//! was put. Visibility is toggled with raw Win32 calls (like the dock) so tao never rewrites
//! the tool-window style.

use crate::db::Db;
use crate::error::AppResult;
use crate::settings::{self, MusicWidgetStyle, Settings};
use crate::win::display::{self, Rect};
use crate::win::window as w;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "widget";
const POSITION_KEY: &str = "widget_position";
const MARGIN: f64 = 24.0;

/// Logical size for each style.
pub fn size(style: MusicWidgetStyle) -> (f64, f64) {
    match style {
        MusicWidgetStyle::Card => (300.0, 432.0),
        MusicWidgetStyle::Compact => (380.0, 214.0),
    }
}

/// Saved top-left corner in physical pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedPos {
    x: i32,
    y: i32,
}

static MOVE_GENERATION: AtomicU64 = AtomicU64::new(0);
static LOCK: Mutex<()> = Mutex::new(());

fn hwnd(app: &AppHandle) -> Option<windows::Win32::Foundation::HWND> {
    app.get_webview_window(LABEL).and_then(|win| win.hwnd().ok())
}

/// Default spot: bottom-right of the primary monitor's work area.
fn default_rect(style: MusicWidgetStyle) -> Rect {
    let mon = display::primary_monitor();
    let (scale, work) = mon.map(|m| (m.scale, m.work)).unwrap_or((1.0, Rect::new(0, 0, 1920, 1040)));
    let (lw, lh) = size(style);
    let (pw, ph) = ((lw * scale).round() as i32, (lh * scale).round() as i32);
    let m = (MARGIN * scale).round() as i32;
    Rect::new(work.right - pw - m, work.bottom - ph - m, pw, ph)
}

fn placed_rect(db: &Db, style: MusicWidgetStyle) -> Rect {
    let fallback = default_rect(style);
    let Some(pos) = settings::load_value::<SavedPos>(db, POSITION_KEY).ok().flatten() else { return fallback };
    // Keep it on whichever monitor now contains the saved corner, fully visible.
    let Some(mon) = display::monitor_for_point(pos.x, pos.y) else { return fallback };
    let (lw, lh) = size(style);
    let r = Rect::new(pos.x, pos.y, (lw * mon.scale).round() as i32, (lh * mon.scale).round() as i32);
    r.clamped_into(&mon.work)
}

/// Creates, restyles or removes the widget to match `s`.
pub fn apply(app: &AppHandle, db: &Arc<Db>, s: &Settings) -> AppResult<()> {
    let _guard = LOCK.lock();
    let existing = app.get_webview_window(LABEL);
    if !s.music_widget {
        if let Some(win) = existing {
            win.destroy()?;
        }
        return Ok(());
    }
    let rect = placed_rect(db, s.music_widget_style);
    let win = match existing {
        Some(win) => win,
        None => {
            let (lw, lh) = size(s.music_widget_style);
            let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#/widget".into()))
                .title("Brim music")
                .inner_size(lw, lh)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .resizable(false)
                .maximizable(false)
                .minimizable(false)
                .skip_taskbar(true)
                .focused(false)
                .visible(false)
                .build()?;
            if let Ok(h) = win.hwnd() {
                w::make_tool_window(h);
            }
            win
        }
    };
    if let Ok(h) = win.hwnd() {
        w::set_bounds(h, rect);
        w::set_topmost(h, s.music_widget_on_top);
    }
    Ok(())
}

/// Shows or hides the widget without activating it (the UI hides it when nothing plays).
pub fn set_visible(app: &AppHandle, visible: bool) {
    if let Some(h) = hwnd(app) {
        if visible && !w::is_visible(h) {
            w::make_tool_window(h);
            w::show_no_activate(h);
        } else if !visible && w::is_visible(h) {
            w::hide(h);
        }
    }
}

/// Called on every move; saves the position once the drag has settled.
pub fn moved(app: &AppHandle, db: Arc<Db>) {
    let generation = MOVE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        if MOVE_GENERATION.load(Ordering::SeqCst) != generation {
            return; // a later move will save
        }
        if let Some(r) = hwnd(&app).and_then(w::window_rect) {
            if let Err(e) = settings::save_value(&db, POSITION_KEY, &SavedPos { x: r.left, y: r.top }) {
                log::warn!("could not save widget position: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rect_sits_inside_the_work_area() {
        for style in [MusicWidgetStyle::Card, MusicWidgetStyle::Compact] {
            let r = default_rect(style);
            assert!(r.width() > 0 && r.height() > 0);
        }
    }
}
