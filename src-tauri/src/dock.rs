//! Dock controller: owns the floating window's geometry, state, focus and placement.
//!
//! All window manipulation happens on one actor thread that receives messages from
//! commands, window events, the tray, the global shortcut and OS events. Main-thread
//! callbacks only *send* messages and never wait, so cross-thread `SetWindowPos` calls
//! cannot deadlock.
//!
//! Geometry model (physical pixels):
//! * collapsed: the window is exactly the compact island — no transparent margins, so the
//!   dock never blocks clicks outside its visible bounds (FR-DCK-008);
//! * expanded: the window is the panel; the island sits at `pill_offset` inside it. The
//!   panel grows downward from the island, or upward when there is no room below.
//!
//! Placement persistence and fallback rule (FR-DCK-005 / FR-DCK-007): the island's position
//! is saved relative to its monitor's work area (centre-x and top as fractions) together with
//! a monitor id. On start and after any display change the island is placed on the saved
//! monitor if it is connected, otherwise on the primary monitor at the same relative
//! position. The saved monitor is not overwritten by a fallback placement, so the dock
//! returns to its monitor when it is reconnected. The island is always clamped fully inside
//! a work area.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::settings::{self, CompactVisibility, PanelStyle, Settings, Utility};
use crate::win::display::{self, Monitor, Rect};
use crate::win::window as w;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const COMPACT_W: f64 = 196.0;
pub const COMPACT_H: f64 = 52.0;
/// Invisible hover strip left where the island was in Vanish mode (centred on the island).
pub const VANISH_W: f64 = 120.0;
pub const VANISH_H: f64 = 16.0;
/// Line mode: hover strip along the island's top edge; the visible line inside it is thinner.
pub const LINE_W: f64 = 196.0;
pub const LINE_H: f64 = 12.0;
/// Live activities: the island grows from its centre into a wider pill for a moment.
pub const NOW_PLAYING_W: f64 = 360.0;
pub const NOW_PLAYING_H: f64 = 64.0;
pub const VOLUME_W: f64 = 260.0;
pub const VOLUME_H: f64 = 52.0;
const NOW_PLAYING_FOR: Duration = Duration::from_millis(5000);
const VOLUME_FOR: Duration = Duration::from_millis(1800);
/// How long the cursor must stay away before the island collapses again.
const UNPEEK_AFTER: Duration = Duration::from_millis(700);
const PEEK_POLL: Duration = Duration::from_millis(150);
/// How long the startup intro holds before the idle collapse can begin.
const INTRO_HOLD: Duration = Duration::from_millis(1700);
/// Expanded panel sizes (logical px) for each layout.
pub const COMPACT_PANEL: (f64, f64) = (400.0, 540.0);
pub const WIDE_PANEL: (f64, f64) = (720.0, 420.0);

pub fn panel_size(style: PanelStyle) -> (f64, f64) {
    match style {
        PanelStyle::Compact => COMPACT_PANEL,
        PanelStyle::Wide => WIDE_PANEL,
    }
}
const EDGE_MARGIN: f64 = 8.0;
const POSITION_KEY: &str = "dock_position";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedPosition {
    pub monitor_id: String,
    pub center_x_frac: f64,
    pub top_frac: f64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Down,
    Up,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockState {
    pub expanded: bool,
    pub utility: Utility,
    pub direction: Direction,
    /// Island position inside the expanded window, in logical (CSS) pixels before dock scaling.
    pub pill_offset_x: f64,
    pub pill_offset_y: f64,
    pub hidden_for_fullscreen: bool,
    /// Collapsed to its idle form (vanished or a line).
    pub idle: bool,
    /// A live activity pill is showing (now playing / volume).
    pub activity: Option<ActivityKind>,
}

/// What the dock collapses into when idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleForm {
    /// Shrinks to a point and disappears (an invisible hover strip remains).
    Vanish,
    /// A thin line along the island's top edge.
    Line,
}

impl IdleForm {
    pub fn from_setting(v: CompactVisibility) -> Option<IdleForm> {
        match v {
            CompactVisibility::Vanish => Some(IdleForm::Vanish),
            CompactVisibility::Line => Some(IdleForm::Line),
            _ => None,
        }
    }
}

pub enum DockMsg {
    Ready,
    GetState(Sender<DockState>),
    Expand { utility: Option<Utility>, reply: Option<Sender<AppResult<DockState>>> },
    Collapse { restore_focus: bool, reply: Option<Sender<AppResult<()>>> },
    Toggle,
    Reveal,
    Focused(bool),
    Moved,
    DisplayChanged,
    FullscreenCheck,
    SettingsChanged(Box<Settings>),
    ModalOpen(bool),
    /// Hover over the dot (true) grows it into the island; false shrinks it back.
    Peek(bool),
    /// Startup intro: show the island and hold it briefly before the idle collapse resumes.
    Intro,
    /// Show a live activity (a song started, the volume changed).
    Activity(ActivityKind),
    /// The UI finished animating an activity away; `to_idle` collapses to the idle form.
    ActivityDone { to_idle: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityKind {
    NowPlaying,
    Volume,
}

#[derive(Clone)]
pub struct DockHandle {
    tx: Sender<DockMsg>,
}

impl DockHandle {
    pub fn send(&self, msg: DockMsg) {
        let _ = self.tx.send(msg);
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> DockMsg) -> AppResult<T> {
        let (tx, rx) = channel();
        self.tx.send(make(tx)).map_err(|_| AppError::internal("dock controller stopped"))?;
        rx.recv_timeout(Duration::from_secs(5)).map_err(|_| AppError::internal("dock controller did not respond"))
    }

    pub fn state(&self) -> AppResult<DockState> {
        self.ask(DockMsg::GetState)
    }

    pub fn expand(&self, utility: Option<Utility>) -> AppResult<DockState> {
        self.ask(|tx| DockMsg::Expand { utility, reply: Some(tx) })?
    }

    pub fn collapse(&self, restore_focus: bool) -> AppResult<()> {
        self.ask(|tx| DockMsg::Collapse { restore_focus, reply: Some(tx) })?
    }
}

/// Pure placement maths, separated for testing.
pub mod geometry {
    use super::*;

    pub fn compact_size(dock_scale: f64, dpi: f64) -> (i32, i32) {
        ((COMPACT_W * dock_scale * dpi).round() as i32, (COMPACT_H * dock_scale * dpi).round() as i32)
    }

    /// A `w`×`h` rectangle centred on (`cx`, `cy`).
    /// The activity pill for an island: centred on it horizontally, sharing its top edge,
    /// kept fully on screen.
    pub fn activity_rect(island: &Rect, kind: ActivityKind, work: &Rect, dock_scale: f64, dpi: f64) -> Rect {
        let (w, h) = match kind {
            ActivityKind::NowPlaying => (NOW_PLAYING_W, NOW_PLAYING_H),
            ActivityKind::Volume => (VOLUME_W, VOLUME_H),
        };
        let (w, h) = ((w * dock_scale * dpi).round() as i32, (h * dock_scale * dpi).round() as i32);
        Rect::new(island.center_x() - w / 2, island.top, w, h).clamped_into(work)
    }

    pub fn around_center(cx: i32, cy: i32, w: i32, h: i32) -> Rect {
        Rect::new(cx - w / 2, cy - h / 2, w, h)
    }

    /// Window rectangle of the idle form for an island at `island`.
    pub fn idle_rect(island: &Rect, form: IdleForm, dock_scale: f64, dpi: f64) -> Rect {
        let px = |v: f64| (v * dock_scale * dpi).round() as i32;
        match form {
            IdleForm::Vanish => around_center(island.center_x(), island.center_y(), px(VANISH_W), px(VANISH_H)),
            IdleForm::Line => Rect::new(island.center_x() - px(LINE_W) / 2, island.top, px(LINE_W), px(LINE_H)),
        }
    }

    /// Inverse of [`idle_rect`]: the island (`w`×`h`) an idle window belongs to. Exact, so
    /// collapsing and expanding never drifts the dock.
    pub fn island_from_idle(idle: &Rect, form: IdleForm, w: i32, h: i32) -> Rect {
        match form {
            IdleForm::Vanish => around_center(idle.center_x(), idle.center_y(), w, h),
            IdleForm::Line => Rect::new(idle.center_x() - w / 2, idle.top, w, h),
        }
    }

    /// Where the island goes on `mon`, given an optional saved relative position.
    pub fn compact_rect(mon: &Monitor, saved: Option<&SavedPosition>, dock_scale: f64) -> Rect {
        let (cw, ch) = compact_size(dock_scale, mon.scale);
        let work = mon.work;
        let (cx, top) = match saved {
            Some(p) if p.center_x_frac.is_finite() && p.top_frac.is_finite() => (
                work.left + (p.center_x_frac.clamp(0.0, 1.0) * work.width() as f64).round() as i32,
                work.top + (p.top_frac.clamp(0.0, 1.0) * work.height() as f64).round() as i32,
            ),
            _ => (work.center_x(), work.top + (EDGE_MARGIN * mon.scale).round() as i32),
        };
        Rect::new(cx - cw / 2, top, cw, ch).clamped_into(&work)
    }

    pub fn saved_from(rect: &Rect, mon: &Monitor) -> SavedPosition {
        let work = mon.work;
        SavedPosition {
            monitor_id: mon.id.clone(),
            center_x_frac: ((rect.center_x() - work.left) as f64 / work.width().max(1) as f64).clamp(0.0, 1.0),
            top_frac: ((rect.top - work.top) as f64 / work.height().max(1) as f64).clamp(0.0, 1.0),
        }
    }

    /// Expanded panel rectangle and direction for an island at `compact` on `mon`.
    pub fn panel_rect(compact: &Rect, mon: &Monitor, dock_scale: f64, size: (f64, f64)) -> (Rect, Direction) {
        let work = mon.work;
        let margin = (EDGE_MARGIN * mon.scale).round() as i32;
        let pw = ((size.0 * dock_scale * mon.scale).round() as i32).min(work.width());
        let ph = ((size.1 * dock_scale * mon.scale).round() as i32).min(work.height() - margin).max(compact.height());
        let left = (compact.center_x() - pw / 2).clamp(work.left, (work.right - pw).max(work.left));
        let (top, dir) = if compact.top + ph <= work.bottom {
            (compact.top, Direction::Down)
        } else if compact.bottom - ph >= work.top {
            (compact.bottom - ph, Direction::Up)
        } else {
            ((work.bottom - ph).max(work.top), Direction::Down)
        };
        (Rect::new(left, top, pw, ph), dir)
    }

    pub fn direction_for(compact: &Rect, mon: &Monitor, dock_scale: f64, size: (f64, f64)) -> Direction {
        panel_rect(compact, mon, dock_scale, size).1
    }
}

struct Dock {
    app: AppHandle,
    hwnd: isize,
    db: Arc<Db>,
    shared_settings: Arc<RwLock<Settings>>,
    settings: Settings,
    ready: bool,
    expanded: bool,
    utility: Utility,
    direction: Direction,
    /// Island offset inside the expanded window, in physical pixels at `offset_dpi`.
    offset: (i32, i32),
    offset_dpi: f64,
    restore_target: Option<isize>,
    hidden_for_fullscreen: bool,
    modal_open: bool,
    saved: Option<SavedPosition>,
    last_set_compact: Option<Rect>,
    save_due: Option<Instant>,
    display_due: Option<Instant>,
    blur_collapse_due: Option<Instant>,
    blur_check_due: Option<Instant>,
    /// Idle form currently applied to the collapsed window (None = the island is shown).
    idle: Option<IdleForm>,
    peek_check_due: Option<Instant>,
    outside_since: Option<Instant>,
    unpeek_fallback_due: Option<Instant>,
    activity: Option<ActivityKind>,
    /// Where the island is while an activity pill is shown (the pill may be clamped on screen).
    activity_island: Option<Rect>,
    activity_until: Option<Instant>,
    activity_end_fallback: Option<Instant>,
}

impl Dock {
    fn hwnd(&self) -> windows::Win32::Foundation::HWND {
        w::hwnd_from(self.hwnd)
    }

    fn state(&self) -> DockState {
        let dpi = self.offset_dpi.max(0.5);
        DockState {
            expanded: self.expanded,
            utility: self.utility,
            direction: self.direction,
            pill_offset_x: self.offset.0 as f64 / dpi,
            pill_offset_y: self.offset.1 as f64 / dpi,
            hidden_for_fullscreen: self.hidden_for_fullscreen,
            idle: !self.expanded && self.idle.is_some(),
            activity: if self.expanded { None } else { self.activity },
        }
    }

    /// The idle form chosen in settings, if the dock collapses when idle.
    fn idle_form(&self) -> Option<IdleForm> {
        IdleForm::from_setting(self.settings.compact_visibility)
    }

    /// The island's rectangle while collapsed, whether the window shows the island or an idle form.
    fn island_rect(&self) -> Option<Rect> {
        self.island_rect_at(self.settings.dock_scale)
    }

    fn island_rect_at(&self, dock_scale: f64) -> Option<Rect> {
        if self.activity.is_some() {
            return self.activity_island;
        }
        let r = w::window_rect(self.hwnd())?;
        let Some(form) = self.idle else { return Some(r) };
        let mon = display::monitor_for_point(r.center_x(), r.center_y()).unwrap_or_else(|| self.current_monitor());
        let (cw, ch) = geometry::compact_size(dock_scale, mon.scale);
        Some(geometry::island_from_idle(&r, form, cw, ch))
    }

    /// Sizes the collapsed window to the island, or to an idle form derived from it.
    fn set_collapsed_bounds(&mut self, island: Rect, form: Option<IdleForm>) {
        let bounds = match form {
            Some(f) => {
                let mon = display::monitor_for_point(island.center_x(), island.center_y())
                    .unwrap_or_else(|| self.current_monitor());
                geometry::idle_rect(&island, f, self.settings.dock_scale, mon.scale)
            }
            None => island,
        };
        w::set_bounds(self.hwnd(), bounds);
        self.idle = form;
        self.last_set_compact = Some(island);
        if form.is_some() {
            self.stop_peek_checks();
        } else {
            self.start_peek_checks();
        }
    }

    /// While the island is shown in a collapsing mode, watch for the cursor leaving it. This
    /// light check runs only while the island is shown; the idle form costs nothing.
    fn start_peek_checks(&mut self) {
        if self.idle_form().is_some() && !self.expanded {
            self.outside_since = None;
            self.peek_check_due = Some(Instant::now() + PEEK_POLL);
        }
    }

    fn stop_peek_checks(&mut self) {
        self.peek_check_due = None;
        self.outside_since = None;
        self.unpeek_fallback_due = None;
    }

    fn peek(&mut self, show: bool) {
        let Some(form) = self.idle_form() else { return };
        if self.expanded || self.activity.is_some() || show != self.idle.is_some() {
            return;
        }
        // Persist a drag that happened while peeking before the window changes shape.
        if self.save_due.take().is_some() {
            self.save_position();
        }
        let Some(island) = self.island_rect() else { return };
        self.set_collapsed_bounds(island, if show { None } else { Some(form) });
        self.emit_state();
    }

    fn activity_enabled(&self, kind: ActivityKind) -> bool {
        match kind {
            ActivityKind::NowPlaying => self.settings.show_now_playing,
            ActivityKind::Volume => self.settings.show_volume_hud,
        }
    }

    /// Grows the island into a live-activity pill without taking focus. Never shown while the
    /// panel is open or a full-screen app is in front.
    fn start_activity(&mut self, kind: ActivityKind) {
        if !self.ready || self.expanded || self.hidden_for_fullscreen || !self.activity_enabled(kind) {
            return;
        }
        let duration = match kind {
            ActivityKind::NowPlaying => NOW_PLAYING_FOR,
            ActivityKind::Volume => VOLUME_FOR,
        };
        // A volume tweak while a song is showing just refreshes the song pill.
        if self.activity == Some(ActivityKind::NowPlaying) && kind == ActivityKind::Volume {
            self.activity_until = Some(Instant::now() + VOLUME_FOR.max(
                self.activity_until.map(|t| t.saturating_duration_since(Instant::now())).unwrap_or_default(),
            ));
            self.activity_end_fallback = None;
            self.emit_state();
            return;
        }
        let Some(island) = self.island_rect() else { return };
        let mon = display::monitor_for_point(island.center_x(), island.center_y()).unwrap_or_else(|| self.current_monitor());
        self.stop_peek_checks();
        let rect = geometry::activity_rect(&island, kind, &mon.work, self.settings.dock_scale, mon.scale);
        w::set_bounds(self.hwnd(), rect);
        self.idle = None;
        self.activity = Some(kind);
        self.activity_island = Some(island);
        self.last_set_compact = Some(island);
        self.activity_until = Some(Instant::now() + duration);
        self.activity_end_fallback = None;
        self.update_visibility();
        if self.settings.always_on_top {
            w::set_topmost(self.hwnd(), true);
        }
        self.emit_state();
    }

    /// Returns from an activity to the island, or to the idle form when `to_idle`.
    fn end_activity(&mut self, to_idle: bool) {
        if self.activity.is_none() {
            return;
        }
        let island = self.activity_island.take();
        self.activity = None;
        self.activity_until = None;
        self.activity_end_fallback = None;
        if let Some(island) = island {
            let form = if to_idle { self.idle_form() } else { None };
            self.set_collapsed_bounds(island, form);
        }
        self.update_visibility();
        self.emit_state();
    }

    /// Drops an activity immediately (display change, panel opening).
    fn clear_activity(&mut self) {
        self.activity = None;
        self.activity_island = None;
        self.activity_until = None;
        self.activity_end_fallback = None;
    }

    fn emit_state(&self) {
        let _ = self.app.emit("dock:state", self.state());
    }

    fn current_monitor(&self) -> Monitor {
        display::monitor_for_window(self.hwnd())
            .or_else(display::primary_monitor)
            .unwrap_or(Monitor {
                id: String::new(),
                handle: 0,
                bounds: Rect::new(0, 0, 1920, 1080),
                work: Rect::new(0, 0, 1920, 1040),
                primary: true,
                scale: 1.0,
            })
    }

    /// Target monitor for the saved position (saved monitor if connected, else primary).
    fn placement_monitor(&self) -> Monitor {
        let monitors = display::monitors();
        if let Some(saved) = &self.saved {
            if let Some(m) = monitors.iter().find(|m| m.id == saved.monitor_id) {
                return m.clone();
            }
        }
        monitors
            .iter()
            .find(|m| m.primary)
            .cloned()
            .or_else(|| monitors.first().cloned())
            .unwrap_or_else(|| self.current_monitor())
    }

    fn place_compact_from_saved(&mut self) {
        let mon = self.placement_monitor();
        let rect = geometry::compact_rect(&mon, self.saved.as_ref(), self.settings.dock_scale);
        let form = self.idle_form();
        self.set_collapsed_bounds(rect, form);
        if form.is_none() {
            self.last_set_compact = w::window_rect(self.hwnd()).or(Some(rect));
        }
        self.direction = geometry::direction_for(&rect, &mon, self.settings.dock_scale, panel_size(self.settings.panel_style));
        let _ = self.app.emit("dock:layout", self.direction);
    }

    fn apply_window_flags(&self) {
        w::make_tool_window(self.hwnd());
        w::set_topmost(self.hwnd(), self.settings.always_on_top);
    }

    fn update_visibility(&mut self) {
        let want = self.ready
            && (self.expanded
                || self.activity.is_some()
                || (self.settings.compact_visibility != CompactVisibility::Hidden && !self.hidden_for_fullscreen));
        let is = w::is_visible(self.hwnd());
        if want && !is {
            self.apply_window_flags();
            w::show_no_activate(self.hwnd());
        } else if !want && is {
            w::hide(self.hwnd());
        }
    }

    fn focus_dock(&self) {
        w::activate(self.hwnd());
        if let Some(win) = self.app.get_webview_window("dock") {
            let webview: &tauri::Webview = win.as_ref();
            let _ = webview.set_focus();
        }
    }

    fn remember_utility(&self, u: Utility) {
        if !self.settings.restore_last_utility {
            return;
        }
        let mut shared = self.shared_settings.write();
        if shared.last_utility != u {
            shared.last_utility = u;
            if let Err(e) = settings::save(&self.db, &shared) {
                log::warn!("could not persist last utility: {e}");
            }
        }
    }

    fn expand(&mut self, utility: Option<Utility>) -> DockState {
        if let Some(u) = utility {
            self.utility = u;
            self.remember_utility(u);
        }
        self.blur_collapse_due = None;
        self.blur_check_due = None;
        if !self.expanded {
            // Remember who had focus so dismissal can hand it back (FR-DCK-009).
            self.restore_target = w::last_external_foreground().map(w::raw);
            self.stop_peek_checks();
            let compact = if w::is_visible(self.hwnd()) {
                self.island_rect().unwrap_or_default()
            } else {
                let mon = self.placement_monitor();
                geometry::compact_rect(&mon, self.saved.as_ref(), self.settings.dock_scale)
            };
            self.clear_activity();
            let mon = display::monitor_for_point(compact.center_x(), compact.center_y()).unwrap_or_else(|| self.current_monitor());
            let (panel, dir) = geometry::panel_rect(&compact, &mon, self.settings.dock_scale, panel_size(self.settings.panel_style));
            self.offset = (compact.left - panel.left, compact.top - panel.top);
            self.offset_dpi = mon.scale;
            self.direction = dir;
            self.expanded = true;
            self.idle = None;
            w::set_bounds(self.hwnd(), panel);
            log::debug!("dock expanded to {panel:?} ({dir:?}), island offset {:?}", self.offset);
            self.update_visibility();
        }
        // Re-assert z-order so the panel is above other topmost windows.
        if self.settings.always_on_top {
            w::set_topmost(self.hwnd(), true);
        }
        self.focus_dock();
        let state = self.state();
        self.emit_state();
        state
    }

    fn collapse(&mut self, restore_focus: bool) {
        self.blur_collapse_due = None;
        self.blur_check_due = None;
        if !self.expanded {
            return;
        }
        let panel = w::window_rect(self.hwnd()).unwrap_or_default();
        let mon = display::monitor_for_point(panel.center_x(), panel.center_y()).unwrap_or_else(|| self.current_monitor());
        // The panel may have been dragged to a monitor with a different DPI.
        let ratio = mon.scale / self.offset_dpi.max(0.5);
        let (cw, ch) = geometry::compact_size(self.settings.dock_scale, mon.scale);
        let compact = Rect::new(
            panel.left + (self.offset.0 as f64 * ratio).round() as i32,
            panel.top + (self.offset.1 as f64 * ratio).round() as i32,
            cw,
            ch,
        )
        .clamped_into(&mon.work);
        self.expanded = false;
        // Collapse to the island; in dot mode it shrinks to the dot once the cursor is away.
        w::set_bounds(self.hwnd(), compact);
        self.idle = None;
        self.start_peek_checks();
        self.direction = geometry::direction_for(&compact, &mon, self.settings.dock_scale, panel_size(self.settings.panel_style));
        if restore_focus {
            if let Some(target) = self.restore_target.take().map(w::hwnd_from) {
                if w::is_valid(target) {
                    w::activate(target);
                }
            }
        }
        self.restore_target = None;
        self.update_visibility();
        // If the panel was dragged, persist the island's new home.
        self.save_due = Some(Instant::now() + Duration::from_millis(300));
        let _ = self.app.emit("dock:layout", self.direction);
        self.emit_state();
    }

    fn save_position(&mut self) {
        if self.expanded {
            return;
        }
        let Some(rect) = self.island_rect() else { return };
        if self.last_set_compact == Some(rect) {
            return; // our own placement (e.g. a fallback) — don't overwrite the saved monitor
        }
        let mon = display::monitor_for_point(rect.center_x(), rect.center_y()).unwrap_or_else(|| self.current_monitor());
        // Keep the island fully visible after a drag that left it partly off-screen.
        let clamped = rect.clamped_into(&mon.work);
        if clamped != rect {
            let form = self.idle;
            self.set_collapsed_bounds(clamped, form);
        }
        self.last_set_compact = Some(clamped);
        let pos = geometry::saved_from(&clamped, &mon);
        if let Err(e) = settings::save_value(&self.db, POSITION_KEY, &pos) {
            log::warn!("could not save dock position: {e}");
        }
        self.saved = Some(pos);
        let dir = geometry::direction_for(&clamped, &mon, self.settings.dock_scale, panel_size(self.settings.panel_style));
        if dir != self.direction {
            self.direction = dir;
        }
        let _ = self.app.emit("dock:layout", self.direction);
    }

    fn check_fullscreen(&mut self) {
        let culprit =
            if self.settings.hide_on_fullscreen { w::foreground_is_fullscreen_on_monitor_of(self.hwnd()) } else { None };
        let fs = culprit.is_some();
        if fs != self.hidden_for_fullscreen {
            self.hidden_for_fullscreen = fs;
            match &culprit {
                Some(app) => log::info!("full-screen app detected ({app}); hiding dock"),
                None => log::info!("full-screen app gone; showing dock"),
            }
            self.update_visibility();
            self.emit_state();
        }
    }

    fn on_display_changed(&mut self) {
        if self.expanded {
            self.collapse(false);
        }
        self.clear_activity();
        self.place_compact_from_saved();
        self.apply_window_flags();
        self.update_visibility();
        self.check_fullscreen();
    }

    fn apply_settings(&mut self, next: Settings) {
        let old = std::mem::replace(&mut self.settings, next);
        if old.always_on_top != self.settings.always_on_top {
            w::set_topmost(self.hwnd(), self.settings.always_on_top);
        }
        if (old.dock_scale - self.settings.dock_scale).abs() > f64::EPSILON {
            if self.expanded {
                self.collapse(false);
            }
            self.end_activity(false);
            // Re-size around the island's centre (the island rect uses the old scale).
            if let Some(old_island) = self.island_rect_at(old.dock_scale) {
                let mon = display::monitor_for_point(old_island.center_x(), old_island.center_y())
                    .unwrap_or_else(|| self.current_monitor());
                let (cw, ch) = geometry::compact_size(self.settings.dock_scale, mon.scale);
                let rect = Rect::new(old_island.center_x() - cw / 2, old_island.top, cw, ch).clamped_into(&mon.work);
                let form = self.idle;
                self.set_collapsed_bounds(rect, form);
                self.direction = geometry::direction_for(&rect, &mon, self.settings.dock_scale, panel_size(self.settings.panel_style));
                let _ = self.app.emit("dock:layout", self.direction);
            }
        }
        if old.compact_visibility != self.settings.compact_visibility && !self.expanded {
            if let Some(island) = self.island_rect() {
                let form = self.idle_form();
                self.set_collapsed_bounds(island, form);
                self.emit_state();
            }
        }
        if old.panel_style != self.settings.panel_style && self.expanded {
            self.collapse(false);
        }
        if old.hide_on_fullscreen != self.settings.hide_on_fullscreen {
            self.check_fullscreen();
        }
        self.update_visibility();
    }

    fn handle(&mut self, msg: DockMsg) {
        let name = match &msg {
            DockMsg::Ready => "ready",
            DockMsg::GetState(_) => "get-state",
            DockMsg::Expand { .. } => "expand",
            DockMsg::Collapse { .. } => "collapse",
            DockMsg::Toggle => "toggle",
            DockMsg::Reveal => "reveal",
            DockMsg::Focused(_) => "focused",
            DockMsg::Moved => "moved",
            DockMsg::DisplayChanged => "display-changed",
            DockMsg::FullscreenCheck => "fullscreen-check",
            DockMsg::SettingsChanged(_) => "settings-changed",
            DockMsg::ModalOpen(_) => "modal",
            DockMsg::Peek(_) => "peek",
            DockMsg::Intro => "intro",
            DockMsg::Activity(_) => "activity",
            DockMsg::ActivityDone { .. } => "activity-done",
        };
        if !matches!(name, "moved" | "fullscreen-check" | "get-state") {
            log::debug!("dock <- {name} (expanded={})", self.expanded);
        }
        match msg {
            DockMsg::Ready => {
                self.ready = true;
                self.apply_window_flags();
                self.check_fullscreen();
                self.update_visibility();
                let _ = self.app.emit("dock:layout", self.direction);
                self.emit_state();
            }
            DockMsg::GetState(reply) => {
                let _ = reply.send(self.state());
            }
            DockMsg::Expand { utility, reply } => {
                let state = self.expand(utility);
                if let Some(r) = reply {
                    let _ = r.send(Ok(state));
                }
            }
            DockMsg::Collapse { restore_focus, reply } => {
                self.collapse(restore_focus);
                if let Some(r) = reply {
                    let _ = r.send(Ok(()));
                }
            }
            DockMsg::Toggle => {
                if self.expanded {
                    // Let the UI animate; fall back to a native collapse if it doesn't respond.
                    let _ = self.app.emit("dock:collapse-request", serde_json::json!({ "restoreFocus": true }));
                    self.blur_collapse_due = Some(Instant::now() + Duration::from_millis(900));
                } else {
                    self.expand(None);
                }
            }
            DockMsg::Reveal => {
                self.expand(None);
            }
            DockMsg::Focused(true) => {
                self.blur_collapse_due = None;
                self.blur_check_due = None;
            }
            DockMsg::Focused(false) => {
                // Focus moving into the embedded webview also reports a blur, so confirm
                // after activation settles that another program really took the foreground.
                if self.expanded && self.settings.collapse_on_blur && !self.modal_open {
                    self.blur_check_due = Some(Instant::now() + Duration::from_millis(150));
                }
            }
            DockMsg::Moved => {
                if !self.expanded {
                    self.save_due = Some(Instant::now() + Duration::from_millis(450));
                }
            }
            DockMsg::DisplayChanged => {
                // Monitors settle over a few hundred milliseconds; coalesce bursts.
                self.display_due = Some(Instant::now() + Duration::from_millis(600));
            }
            DockMsg::FullscreenCheck => self.check_fullscreen(),
            DockMsg::SettingsChanged(next) => self.apply_settings(*next),
            DockMsg::ModalOpen(open) => {
                self.modal_open = open;
                if open {
                    self.blur_collapse_due = None;
                }
            }
            DockMsg::Peek(show) => self.peek(show),
            DockMsg::Intro => {
                // Grow out of the idle form if collapsed; the UI plays the intro. Delay the
                // hover-out check so the intro finishes before the island collapses again.
                self.peek(true);
                if self.idle.is_none() && self.idle_form().is_some() && !self.expanded {
                    self.outside_since = None;
                    self.peek_check_due = Some(Instant::now() + INTRO_HOLD);
                }
            }
            DockMsg::Activity(kind) => self.start_activity(kind),
            DockMsg::ActivityDone { to_idle } => self.end_activity(to_idle),
        }
    }

    fn run_timers(&mut self) {
        let now = Instant::now();
        if self.display_due.is_some_and(|t| t <= now) {
            self.display_due = None;
            self.on_display_changed();
        }
        if self.save_due.is_some_and(|t| t <= now) {
            self.save_due = None;
            self.save_position();
        }
        if self.blur_check_due.is_some_and(|t| t <= now) {
            self.blur_check_due = None;
            let fg = w::foreground();
            let ours = fg == self.hwnd() || w::process_id(fg) == std::process::id();
            if self.expanded && !self.modal_open && !ours && !fg.0.is_null() {
                let _ = self.app.emit("dock:collapse-request", serde_json::json!({ "restoreFocus": false }));
                self.blur_collapse_due = Some(Instant::now() + Duration::from_millis(900));
            }
        }
        if self.blur_collapse_due.is_some_and(|t| t <= now) {
            self.blur_collapse_due = None;
            if self.expanded && !self.modal_open {
                self.collapse(false);
            }
        }
        if self.peek_check_due.is_some_and(|t| t <= now) {
            self.peek_check_due = None;
            if !self.expanded
                && self.activity.is_none()
                && self.idle.is_none()
                && self.idle_form().is_some()
                && self.unpeek_fallback_due.is_none()
            {
                if w::cursor_inside(self.hwnd()) {
                    self.outside_since = None;
                } else if now.duration_since(*self.outside_since.get_or_insert(now)) >= UNPEEK_AFTER {
                    // Let the UI animate the island back into the dot; fall back natively.
                    let _ = self.app.emit("dock:unpeek-request", ());
                    self.unpeek_fallback_due = Some(now + Duration::from_millis(800));
                }
                if self.unpeek_fallback_due.is_none() {
                    self.peek_check_due = Some(now + PEEK_POLL);
                }
            }
        }
        if self.unpeek_fallback_due.is_some_and(|t| t <= now) {
            self.unpeek_fallback_due = None;
            self.peek(false);
        }
        if self.activity.is_some() && self.activity_end_fallback.is_none() && self.activity_until.is_some_and(|t| t <= now) {
            if w::cursor_inside(self.hwnd()) {
                // Keep it up while the pointer is on it.
                self.activity_until = Some(now + Duration::from_millis(600));
            } else {
                let to_idle = self.idle_form().is_some();
                let _ = self.app.emit("dock:activity-end-request", serde_json::json!({ "toIdle": to_idle }));
                self.activity_end_fallback = Some(now + Duration::from_millis(800));
            }
        }
        if self.activity_end_fallback.is_some_and(|t| t <= now) {
            let to_idle = self.idle_form().is_some();
            self.end_activity(to_idle);
        }
    }

    fn next_timeout(&self) -> Duration {
        [
            self.display_due,
            self.save_due,
            self.blur_collapse_due,
            self.blur_check_due,
            self.peek_check_due,
            self.unpeek_fallback_due,
            if self.activity_end_fallback.is_none() { self.activity_until } else { None },
            self.activity_end_fallback,
        ]
            .into_iter()
            .flatten()
            .min()
            .map(|t| t.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600))
    }
}

pub fn start(app: AppHandle, hwnd: isize, db: Arc<Db>, shared_settings: Arc<RwLock<Settings>>) -> DockHandle {
    let (tx, rx) = channel();
    let settings = shared_settings.read().clone();
    let saved: Option<SavedPosition> = settings::load_value(&db, POSITION_KEY).unwrap_or(None);
    let utility = if settings.restore_last_utility { settings.last_utility } else { Utility::Music };
    let mut dock = Dock {
        app,
        hwnd,
        db,
        shared_settings,
        settings,
        ready: false,
        expanded: false,
        utility,
        direction: Direction::Down,
        offset: (0, 0),
        offset_dpi: 1.0,
        restore_target: None,
        hidden_for_fullscreen: false,
        modal_open: false,
        saved,
        last_set_compact: None,
        save_due: None,
        display_due: None,
        blur_collapse_due: None,
        blur_check_due: None,
        idle: None,
        peek_check_due: None,
        outside_since: None,
        unpeek_fallback_due: None,
        activity: None,
        activity_island: None,
        activity_until: None,
        activity_end_fallback: None,
    };
    dock.apply_window_flags();
    dock.place_compact_from_saved();
    std::thread::Builder::new()
        .name("dock".into())
        .spawn(move || run(dock, rx))
        .expect("spawn dock thread");
    DockHandle { tx }
}

fn run(mut dock: Dock, rx: Receiver<DockMsg>) {
    loop {
        let timeout = dock.next_timeout();
        match rx.recv_timeout(timeout) {
            Ok(msg) => {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dock.handle(msg)));
                if result.is_err() {
                    log::error!("dock controller recovered from a panic while handling a message");
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| dock.run_timers()));
        if result.is_err() {
            log::error!("dock controller recovered from a panic in timers");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::geometry::*;
    use super::*;

    fn mon(id: &str, left: i32, w: i32, h: i32, scale: f64) -> Monitor {
        Monitor {
            id: id.into(),
            handle: 1,
            bounds: Rect::new(left, 0, w, h),
            work: Rect::new(left, 0, w, h - 40),
            primary: left == 0,
            scale,
        }
    }

    #[test]
    fn default_placement_is_top_centre() {
        let m = mon("a", 0, 1920, 1080, 1.0);
        let r = compact_rect(&m, None, 1.0);
        assert_eq!((r.width(), r.height()), (196, 52));
        assert_eq!(r.center_x(), 960);
        assert_eq!(r.top, 8);
    }

    #[test]
    fn saved_position_round_trips_and_scales_with_resolution() {
        let m = mon("a", 0, 1920, 1080, 1.0);
        let r = Rect::new(1500, 600, 196, 52);
        let saved = saved_from(&r, &m);
        let back = compact_rect(&m, Some(&saved), 1.0);
        assert!((back.left - r.left).abs() <= 1 && (back.top - r.top).abs() <= 1);
        // Same relative spot on a larger monitor; size follows DPI.
        let big = mon("a", 0, 3840, 2160, 2.0);
        let moved = compact_rect(&big, Some(&saved), 1.0);
        assert_eq!((moved.width(), moved.height()), (392, 104));
        assert!(big.work.contains_rect(&moved));
    }

    #[test]
    fn placement_is_always_fully_visible() {
        let m = mon("a", 0, 1366, 768, 1.25);
        let saved = SavedPosition { monitor_id: "a".into(), center_x_frac: 1.0, top_frac: 1.0 };
        let r = compact_rect(&m, Some(&saved), 1.3);
        assert!(m.work.contains_rect(&r));
        let nan = SavedPosition { monitor_id: "a".into(), center_x_frac: f64::NAN, top_frac: 0.5 };
        assert!(m.work.contains_rect(&compact_rect(&m, Some(&nan), 1.0)));
    }

    #[test]
    fn panel_expands_down_or_up_and_stays_on_screen() {
        let m = mon("a", 0, 1920, 1080, 1.0);
        let top_pill = Rect::new(862, 8, 196, 52);
        let (p, d) = panel_rect(&top_pill, &m, 1.0, COMPACT_PANEL);
        assert_eq!(d, Direction::Down);
        assert_eq!((p.top, p.center_x()), (8, 960));
        assert!(m.work.contains_rect(&p));

        let bottom_pill = Rect::new(862, 980, 196, 52);
        let (p, d) = panel_rect(&bottom_pill, &m, 1.0, COMPACT_PANEL);
        assert_eq!(d, Direction::Up);
        assert_eq!(p.bottom, bottom_pill.bottom);
        assert!(m.work.contains_rect(&p));

        // Hugging the right edge: panel is clamped, island offset is non-centred.
        let edge = Rect::new(1920 - 196, 8, 196, 52);
        let (p, _) = panel_rect(&edge, &m, 1.0, COMPACT_PANEL);
        assert_eq!(p.right, 1920);
        assert_eq!(edge.left - p.left, 400 - 196);
    }

    #[test]
    fn idle_forms_round_trip_without_drift() {
        for (w, h, scale, dpi) in [(196, 52, 1.0, 1.0), (197, 53, 1.0, 1.25), (225, 60, 1.15, 1.5)] {
            let island = Rect::new(861, 9, w, h);
            for form in [IdleForm::Vanish, IdleForm::Line] {
                let idle = idle_rect(&island, form, scale, dpi);
                assert_eq!(island_from_idle(&idle, form, w, h), island, "{form:?}");
                assert!(idle.top >= island.top && idle.bottom <= island.bottom, "{form:?} stays within the island's rows");
            }
            // The line sits on the island's top edge.
            assert_eq!(idle_rect(&island, IdleForm::Line, scale, dpi).top, island.top);
        }
    }

    #[test]
    fn activity_pill_grows_from_island_centre_and_stays_on_screen() {
        let m = mon("a", 0, 1920, 1080, 1.0);
        let island = Rect::new(862, 8, 196, 52);
        let r = activity_rect(&island, ActivityKind::NowPlaying, &m.work, 1.0, 1.0);
        assert_eq!((r.center_x(), r.top, r.width(), r.height()), (island.center_x(), island.top, 360, 64));
        let edge = Rect::new(1920 - 196, 8, 196, 52);
        let r = activity_rect(&edge, ActivityKind::NowPlaying, &m.work, 1.0, 1.0);
        assert!(m.work.contains_rect(&r));
    }

    #[test]
    fn wide_panel_is_centred_under_the_island() {
        let m = mon("a", 0, 1920, 1080, 1.0);
        let pill = Rect::new(862, 8, 196, 52);
        let (p, d) = panel_rect(&pill, &m, 1.0, WIDE_PANEL);
        assert_eq!((p.width(), p.height(), p.center_x(), p.top, d), (720, 420, 960, 8, Direction::Down));
    }

    #[test]
    fn panel_fits_small_screens() {
        let m = mon("a", 0, 1024, 600, 1.0); // work area 560 tall
        let pill = Rect::new(400, 200, 196, 52);
        let (p, _) = panel_rect(&pill, &m, 1.3, COMPACT_PANEL);
        assert!(p.height() <= m.work.height());
        assert!(m.work.contains_rect(&p));
    }
}
