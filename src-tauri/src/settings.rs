//! Application settings: typed model, safe defaults, validation and persistence.
//!
//! Settings are stored as one JSON document under key `app` in the `settings` table.
//! `#[serde(default)]` means fields added in later versions get their defaults when an
//! older document is loaded, so updates never lose or break existing preferences.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Utility {
    Music,
    Clipboard,
    Apps,
    Tasks,
    Calendar,
    Snippets,
    Wallpapers,
    /// Media + volume ("Controls" tab of the wide layout).
    Controls,
    /// Today's tasks, reminders and events.
    Today,
    /// CPU, memory, disk, battery, uptime.
    Stats,
    /// Lock, sleep, restart, shut down, sign out.
    Power,
    /// YouTube / web search.
    Search,
}

impl Utility {
    pub const ALL: [Utility; 12] = [
        Utility::Music,
        Utility::Clipboard,
        Utility::Apps,
        Utility::Tasks,
        Utility::Calendar,
        Utility::Snippets,
        Utility::Wallpapers,
        Utility::Controls,
        Utility::Today,
        Utility::Stats,
        Utility::Power,
        Utility::Search,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Utility::Music => "music",
            Utility::Clipboard => "clipboard",
            Utility::Apps => "apps",
            Utility::Tasks => "tasks",
            Utility::Calendar => "calendar",
            Utility::Snippets => "snippets",
            Utility::Wallpapers => "wallpapers",
            Utility::Controls => "controls",
            Utility::Today => "today",
            Utility::Stats => "stats",
            Utility::Power => "power",
            Utility::Search => "search",
        }
    }

    pub fn parse(s: &str) -> Option<Utility> {
        Utility::ALL.into_iter().find(|u| u.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Utility::Music => "Music",
            Utility::Clipboard => "Clipboard",
            Utility::Apps => "Apps",
            Utility::Tasks => "Tasks",
            Utility::Calendar => "Calendar",
            Utility::Snippets => "Snippets",
            Utility::Wallpapers => "Wallpapers",
            Utility::Controls => "Controls",
            Utility::Today => "Today",
            Utility::Stats => "Stats",
            Utility::Power => "Power",
            Utility::Search => "Search",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
    /// Near-black with an amber accent.
    Midnight,
    /// Aqua-to-white gradient with a glossy sheen.
    Aqua,
    System,
}

/// Layout of the expanded panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelStyle {
    /// Narrow panel with the four slot icons in its header.
    Compact,
    /// Wide panel with a row of text tabs.
    Wide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnimationLevel {
    Full,
    Reduced,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactVisibility {
    /// Idle dock collapses to a point and disappears; hovering where it was brings it back.
    /// (`dot` was the name used by an earlier build.)
    #[serde(alias = "dot")]
    Vanish,
    /// Idle dock collapses into a thin line at its top edge; hovering the line brings it back.
    Line,
    /// Compact island always visible.
    Always,
    /// Compact island fades when the pointer is away.
    Fade,
    /// Compact island hidden; the dock appears only via shortcut or tray.
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureState {
    On,
    Paused,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperFit {
    Fill,
    Fit,
    Stretch,
    Center,
    Tile,
    Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MusicWidgetStyle {
    /// Tall card with large artwork.
    Card,
    /// Wide card with artwork beside the title.
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayClickAction {
    Toggle,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    // Appearance
    pub theme: Theme,
    pub accent_color: String,
    pub surface_opacity: f64,
    pub corner_radius: u32,
    pub dock_scale: f64,
    pub panel_style: PanelStyle,
    pub animation_level: AnimationLevel,
    // Behaviour
    pub always_on_top: bool,
    pub hide_on_fullscreen: bool,
    pub collapse_on_blur: bool,
    pub expand_on_scroll: bool,
    pub compact_visibility: CompactVisibility,
    pub shortcut: String,
    /// True once the user picks a shortcut; until then a taken default may be replaced.
    pub shortcut_customized: bool,
    pub collapse_after_copy: bool,
    pub collapse_after_launch: bool,
    pub tray_click_action: TrayClickAction,
    /// Which one-time default upgrades have been applied to this settings document.
    /// Field-level default 0 so documents saved before it existed are upgraded once.
    #[serde(default)]
    pub defaults_version: u32,
    // Startup / state
    pub restore_last_utility: bool,
    pub last_utility: Utility,
    pub onboarding_completed: bool,
    // Clipboard
    pub clipboard_capture: CaptureState,
    pub clipboard_paused_until: Option<i64>,
    pub clipboard_retention_days: u32,
    pub clipboard_max_items: u32,
    pub clipboard_max_entry_kb: u32,
    pub clipboard_skip_secrets: bool,
    pub clipboard_ignored_apps: Vec<String>,
    // Live activities
    pub show_now_playing: bool,
    pub show_volume_hud: bool,
    /// Handle the keyboard volume keys in the dock (hides Windows' volume pop-up).
    pub handle_volume_keys: bool,
    // Desktop music widget
    pub music_widget: bool,
    pub music_widget_style: MusicWidgetStyle,
    pub music_widget_on_top: bool,
    pub music_widget_hide_idle: bool,
    // Launcher
    pub track_recent_apps: bool,
    // Notifications
    pub notifications_enabled: bool,
    // Wallpapers
    pub wallpaper_fit: WallpaperFit,
}

/// Bump when a new default should be applied once to existing installs.
pub const DEFAULTS_VERSION: u32 = 3;

pub const DEFAULT_SHORTCUT: &str = "Alt+Shift+Space";
/// Tried in order at startup when the user has never chosen a shortcut and the default is
/// already taken by another program, so the dock always has a working shortcut.
pub const FALLBACK_SHORTCUTS: [&str; 3] = ["Ctrl+Alt+Backquote", "Ctrl+Shift+Space", "Alt+Backquote"];

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Midnight,
            accent_color: "#7c8cff".into(),
            surface_opacity: 0.86,
            corner_radius: 20,
            dock_scale: 1.0,
            panel_style: PanelStyle::Wide,
            animation_level: AnimationLevel::Full,
            always_on_top: true,
            hide_on_fullscreen: true,
            collapse_on_blur: true,
            expand_on_scroll: true,
            compact_visibility: CompactVisibility::Vanish,
            shortcut: DEFAULT_SHORTCUT.into(),
            shortcut_customized: false,
            collapse_after_copy: true,
            collapse_after_launch: true,
            tray_click_action: TrayClickAction::Toggle,
            defaults_version: DEFAULTS_VERSION,
            restore_last_utility: true,
            last_utility: Utility::Controls,
            onboarding_completed: false,
            // Capture is OFF until the user makes an explicit choice during first run.
            clipboard_capture: CaptureState::Off,
            clipboard_paused_until: None,
            clipboard_retention_days: 7,
            clipboard_max_items: 200,
            clipboard_max_entry_kb: 256,
            clipboard_skip_secrets: true,
            clipboard_ignored_apps: vec![
                "keepass.exe".into(),
                "keepassxc.exe".into(),
                "1password.exe".into(),
                "bitwarden.exe".into(),
                "dashlane.exe".into(),
                "lastpass.exe".into(),
                "enpass.exe".into(),
                "proton pass.exe".into(),
            ],
            show_now_playing: true,
            show_volume_hud: true,
            handle_volume_keys: true,
            music_widget: false,
            music_widget_style: MusicWidgetStyle::Card,
            music_widget_on_top: false,
            music_widget_hide_idle: true,
            track_recent_apps: true,
            notifications_enabled: true,
            wallpaper_fit: WallpaperFit::Fill,
        }
    }
}

impl Settings {
    /// Validates every field. Returns a user-facing error naming the first problem.
    pub fn validate(&mut self) -> AppResult<()> {
        let hex = self.accent_color.trim();
        if hex.len() != 7 || !hex.starts_with('#') || !hex[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AppError::invalid("Accent colour must be a hex colour like #7c8cff."));
        }
        self.accent_color = hex.to_ascii_lowercase();
        if !(0.5..=1.0).contains(&self.surface_opacity) || !self.surface_opacity.is_finite() {
            return Err(AppError::invalid("Opacity must be between 50% and 100%."));
        }
        if self.corner_radius > 32 {
            return Err(AppError::invalid("Corner radius must be 32 or less."));
        }
        if !(0.8..=1.3).contains(&self.dock_scale) || !self.dock_scale.is_finite() {
            return Err(AppError::invalid("Dock size must be between 80% and 130%."));
        }
        self.shortcut = self.shortcut.trim().to_string();
        if self.shortcut.is_empty() || self.shortcut.len() > 64 {
            return Err(AppError::invalid("Keyboard shortcut is not valid."));
        }
        if self.clipboard_retention_days > 365 {
            return Err(AppError::invalid("Clipboard retention must be 365 days or fewer."));
        }
        if !(10..=5000).contains(&self.clipboard_max_items) {
            return Err(AppError::invalid("Clipboard history size must be between 10 and 5000 items."));
        }
        if !(1..=2048).contains(&self.clipboard_max_entry_kb) {
            return Err(AppError::invalid("Per-item clipboard limit must be between 1 KB and 2 MB."));
        }
        if self.clipboard_capture != CaptureState::Paused {
            self.clipboard_paused_until = None;
        }
        let mut apps = Vec::new();
        for raw in &self.clipboard_ignored_apps {
            let name = raw.trim().to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            if name.len() > 128 || name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) || !name.ends_with(".exe") {
                return Err(AppError::invalid(format!(
                    "\"{}\" is not a valid program name. Use the executable name, e.g. keepass.exe.",
                    crate::util::truncate_chars(raw.trim(), 40)
                )));
            }
            if !apps.contains(&name) {
                apps.push(name);
            }
        }
        if apps.len() > 50 {
            return Err(AppError::invalid("You can ignore up to 50 programs."));
        }
        self.clipboard_ignored_apps = apps;
        Ok(())
    }

    /// Effective capture state, taking a timed pause into account.
    pub fn capture_active(&self, now: i64) -> bool {
        match self.clipboard_capture {
            CaptureState::On => true,
            CaptureState::Off => false,
            CaptureState::Paused => matches!(self.clipboard_paused_until, Some(until) if until <= now),
        }
    }

    /// Applies a JSON patch of camelCase fields on top of `self`. Unknown keys are rejected.
    pub fn patched(&self, patch: &serde_json::Value) -> AppResult<Settings> {
        let obj = patch.as_object().ok_or_else(|| AppError::invalid("Settings update must be an object."))?;
        let mut current = serde_json::to_value(self)?;
        let target = current.as_object_mut().expect("settings serialise to an object");
        for (key, value) in obj {
            if !target.contains_key(key) {
                return Err(AppError::invalid(format!("Unknown setting \"{key}\".")));
            }
            target.insert(key.clone(), value.clone());
        }
        let mut next: Settings = serde_json::from_value(current)
            .map_err(|e| AppError::invalid("One of the settings has an invalid value.").with_detail(e.to_string()))?;
        next.validate()?;
        Ok(next)
    }
}

const SETTINGS_KEY: &str = "app";

pub fn load(db: &Db) -> AppResult<Settings> {
    let raw: Option<String> = db.with(|c| {
        Ok(c.query_row("SELECT value FROM settings WHERE key = ?1", [SETTINGS_KEY], |r| r.get(0)).optional()?)
    })?;
    let mut settings = match raw {
        Some(json) => serde_json::from_str::<Settings>(&json).unwrap_or_else(|e| {
            log::error!("settings document unreadable, using defaults: {e}");
            Settings::default()
        }),
        None => Settings::default(),
    };
    if settings.validate().is_err() {
        log::warn!("stored settings failed validation; falling back to defaults for safety");
        settings = Settings::default();
    }
    if settings.defaults_version < 1 {
        // v1: the idle dock collapses out of the way by default.
        settings.compact_visibility = CompactVisibility::Vanish;
    }
    if settings.defaults_version < 2 {
        // v2: the Aqua theme becomes the look of the dock.
        settings.theme = Theme::Aqua;
    }
    if settings.defaults_version < 3 {
        // v3: the wide, midnight-and-amber layout.
        settings.theme = Theme::Midnight;
        settings.panel_style = PanelStyle::Wide;
    }
    if settings.defaults_version < DEFAULTS_VERSION {
        settings.defaults_version = DEFAULTS_VERSION;
        save(db, &settings)?;
    }
    Ok(settings)
}

pub fn save(db: &Db, settings: &Settings) -> AppResult<()> {
    let json = serde_json::to_string(settings)?;
    db.with(|c| {
        c.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [SETTINGS_KEY, json.as_str()],
        )?;
        Ok(())
    })
}

pub fn load_value<T: for<'de> Deserialize<'de>>(db: &Db, key: &str) -> AppResult<Option<T>> {
    let raw: Option<String> =
        db.with(|c| Ok(c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?))?;
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn save_value<T: Serialize>(db: &Db, key: &str, value: &T) -> AppResult<()> {
    let json = serde_json::to_string(value)?;
    db.with(|c| {
        c.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, json.as_str()],
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_are_valid_and_capture_is_off() {
        let mut s = Settings::default();
        s.validate().unwrap();
        assert_eq!(s.clipboard_capture, CaptureState::Off);
        assert!(!s.onboarding_completed);
    }

    #[test]
    fn patch_rejects_unknown_and_invalid() {
        let s = Settings::default();
        assert!(s.patched(&json!({"nope": 1})).is_err());
        assert!(s.patched(&json!({"surfaceOpacity": 0.1})).is_err());
        assert!(s.patched(&json!({"accentColor": "red"})).is_err());
        assert!(s.patched(&json!({"clipboardIgnoredApps": ["C:\\evil.exe"]})).is_err());
        let next = s.patched(&json!({"theme": "light", "accentColor": "#AABBCC"})).unwrap();
        assert_eq!(next.theme, Theme::Light);
        assert_eq!(next.accent_color, "#aabbcc");
    }

    #[test]
    fn ignored_apps_are_normalised_and_deduplicated() {
        let s = Settings::default();
        let next = s.patched(&json!({"clipboardIgnoredApps": [" KeePass.exe ", "keepass.exe", ""]})).unwrap();
        assert_eq!(next.clipboard_ignored_apps, vec!["keepass.exe".to_string()]);
    }

    #[test]
    fn timed_pause_expires() {
        let s = Settings { clipboard_capture: CaptureState::Paused, clipboard_paused_until: Some(100), ..Settings::default() };
        assert!(!s.capture_active(50));
        assert!(s.capture_active(100));
        let indefinite = Settings { clipboard_capture: CaptureState::Paused, clipboard_paused_until: None, ..Settings::default() };
        assert!(!indefinite.capture_active(i64::MAX));
    }

    #[test]
    fn one_time_default_upgrade_switches_to_vanish_once() {
        let db = Db::open_in_memory().unwrap();
        // A document saved before defaults_version existed, with the old default.
        db.with(|c| {
            c.execute("INSERT INTO settings (key, value) VALUES ('app', '{\"compactVisibility\":\"always\"}')", [])?;
            Ok(())
        })
        .unwrap();
        let s = load(&db).unwrap();
        assert_eq!(s.compact_visibility, CompactVisibility::Vanish);
        assert_eq!(s.theme, Theme::Midnight);
        assert_eq!(s.panel_style, PanelStyle::Wide);
        // The earlier "dot" value is read as Vanish.
        let old: Settings = serde_json::from_str(r#"{"compactVisibility":"dot"}"#).unwrap();
        assert_eq!(old.compact_visibility, CompactVisibility::Vanish);
        assert_eq!(s.defaults_version, DEFAULTS_VERSION);
        // A later explicit choice is respected.
        save(&db, &Settings { compact_visibility: CompactVisibility::Always, ..s }).unwrap();
        assert_eq!(load(&db).unwrap().compact_visibility, CompactVisibility::Always);
    }

    #[test]
    fn older_documents_gain_new_fields() {
        let s: Settings = serde_json::from_str(r#"{"theme":"light"}"#).unwrap();
        assert_eq!(s.theme, Theme::Light);
        assert_eq!(s.shortcut, DEFAULT_SHORTCUT);
    }

    #[test]
    fn persistence_round_trip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(load(&db).unwrap(), Settings::default());
        let s = Settings { theme: Theme::Light, dock_scale: 1.15, ..Settings::default() };
        save(&db, &s).unwrap();
        assert_eq!(load(&db).unwrap(), s);
    }
}
