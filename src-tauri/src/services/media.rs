//! Windows media session integration (Global System Media Transport Controls).
//!
//! Runs on a dedicated MTA thread and is entirely event-driven: the manager's
//! `SessionsChanged` / `CurrentSessionChanged` and each session's property, playback
//! and timeline events trigger a debounced refresh. Calls into players go through a
//! watchdog so an unresponsive player can never block the dock.
//!
//! Active-session selection rule (documented in docs/COMPATIBILITY.md):
//! 1. the session the user picked, while it still exists;
//! 2. otherwise the session Windows reports as current;
//! 3. otherwise the first session that is playing;
//! 4. otherwise the first session.

use crate::error::{AppError, AppResult};
use crate::util::data_url;
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

const CALL_TIMEOUT: Duration = Duration::from_millis(2500);
const MAX_ARTWORK_BYTES: u64 = 2 * 1024 * 1024;
/// Windows FILETIME epoch (1601) to Unix epoch, in 100 ns ticks.
const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub app_name: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub session_id: String,
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: Option<String>,
    pub status: &'static str,
    pub can_play_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
    pub position_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub position_updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaState {
    /// False when the Windows media session service is unavailable (e.g. N editions).
    pub available: bool,
    pub sessions: Vec<SessionInfo>,
    pub active: Option<NowPlaying>,
    pub manual_selection: bool,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
}

enum Msg {
    Refresh,
    Command(MediaCommand, Sender<AppResult<()>>),
    Seek(i64, Sender<AppResult<()>>),
    Select(Option<String>, Sender<AppResult<()>>),
}

pub type Emit = Arc<dyn Fn(&MediaState) + Send + Sync>;

pub struct MediaService {
    tx: Sender<Msg>,
    state: Arc<RwLock<MediaState>>,
}

impl MediaService {
    pub fn start(emit: Emit) -> Self {
        let (tx, rx) = channel();
        let state = Arc::new(RwLock::new(MediaState::default()));
        let thread_tx = tx.clone();
        let thread_state = state.clone();
        std::thread::Builder::new()
            .name("media".into())
            .spawn(move || {
                // A panic here must not take the app down; the panel will show "unavailable".
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(thread_tx, rx, thread_state.clone(), emit.clone())
                }));
                if result.is_err() {
                    log::error!("media thread panicked; media controls disabled for this session");
                    let mut s = thread_state.write();
                    *s = MediaState::default();
                    emit(&s);
                }
            })
            .expect("spawn media thread");
        Self { tx, state }
    }

    pub fn state(&self) -> MediaState {
        self.state.read().clone()
    }

    pub fn refresh(&self) {
        let _ = self.tx.send(Msg::Refresh);
    }

    fn request(&self, make: impl FnOnce(Sender<AppResult<()>>) -> Msg) -> AppResult<()> {
        let (tx, rx) = channel();
        self.tx.send(make(tx)).map_err(|_| unavailable())?;
        rx.recv_timeout(CALL_TIMEOUT + Duration::from_secs(1)).map_err(|_| {
            AppError::new("media_timeout", "The media player did not respond.")
        })?
    }

    pub fn command(&self, cmd: MediaCommand) -> AppResult<()> {
        self.request(|tx| Msg::Command(cmd, tx))
    }

    pub fn seek(&self, position_ms: i64) -> AppResult<()> {
        if position_ms < 0 {
            return Err(AppError::invalid("Position must not be negative."));
        }
        self.request(|tx| Msg::Seek(position_ms, tx))
    }

    pub fn select(&self, session_id: Option<String>) -> AppResult<()> {
        self.request(|tx| Msg::Select(session_id, tx))
    }
}

fn unavailable() -> AppError {
    AppError::new("media_unavailable", "Media controls are not available right now.")
}

/// Runs `f` on a helper thread and waits at most `CALL_TIMEOUT`.
fn with_timeout<T: Send + 'static>(f: impl FnOnce() -> windows::core::Result<T> + Send + 'static) -> Option<T> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let _ = tx.send(f());
    });
    match rx.recv_timeout(CALL_TIMEOUT) {
        Ok(Ok(v)) => Some(v),
        Ok(Err(e)) => {
            log::debug!("media call failed: {}", e.message());
            None
        }
        Err(_) => {
            log::warn!("media call timed out");
            None
        }
    }
}

pub fn friendly_app_name(aumid: &str) -> String {
    let lower = aumid.to_lowercase();
    const KNOWN: [(&str, &str); 18] = [
        ("spotify", "Spotify"),
        ("msedge", "Microsoft Edge"),
        ("chrome", "Google Chrome"),
        ("firefox", "Firefox"),
        ("308046b0af4a39cb", "Firefox"),
        ("zunemusic", "Media Player"),
        ("zunevideo", "Movies & TV"),
        ("brave", "Brave"),
        ("opera", "Opera"),
        ("vivaldi", "Vivaldi"),
        ("vlc", "VLC"),
        ("foobar2000", "foobar2000"),
        ("applemusic", "Apple Music"),
        ("itunes", "iTunes"),
        ("musicbee", "MusicBee"),
        ("tidal", "TIDAL"),
        ("deezer", "Deezer"),
        ("discord", "Discord"),
    ];
    if let Some((_, name)) = KNOWN.iter().find(|(k, _)| lower.contains(k)) {
        return name.to_string();
    }
    let tail = aumid.rsplit('!').next().unwrap_or(aumid);
    let tail = tail.strip_suffix(".exe").or_else(|| tail.strip_suffix(".EXE")).unwrap_or(tail);
    let tail = tail.split('_').next().unwrap_or(tail);
    let tail = tail.rsplit('.').next().unwrap_or(tail);
    let mut chars = tail.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Media app".into(),
    }
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Playing => "playing",
        Status::Paused => "paused",
        Status::Stopped => "stopped",
        Status::Changing => "changing",
        Status::Opened => "opened",
        _ => "closed",
    }
}

fn ticks_to_unix_ms(ticks: i64) -> Option<i64> {
    if ticks <= EPOCH_DIFF_TICKS {
        None
    } else {
        Some((ticks - EPOCH_DIFF_TICKS) / 10_000)
    }
}

struct Props {
    title: String,
    artist: String,
    album: String,
}

fn read_props(session: &Session) -> Option<Props> {
    let s = session.clone();
    with_timeout(move || {
        let p = s.TryGetMediaPropertiesAsync()?.join()?;
        Ok(Props {
            title: p.Title().map(|h| h.to_string()).unwrap_or_default(),
            artist: p.Artist().map(|h| h.to_string()).unwrap_or_default(),
            album: p.AlbumTitle().map(|h| h.to_string()).unwrap_or_default(),
        })
    })
}

fn read_artwork(session: &Session) -> Option<String> {
    let s = session.clone();
    with_timeout(move || {
        let p = s.TryGetMediaPropertiesAsync()?.join()?;
        let reference = p.Thumbnail()?;
        let stream = reference.OpenReadAsync()?.join()?;
        let size = stream.Size()?;
        if size == 0 || size > MAX_ARTWORK_BYTES {
            return Ok(None);
        }
        let mime = stream.ContentType().map(|h| h.to_string()).unwrap_or_default();
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        let loaded = reader.LoadAsync(size as u32)?.join()?;
        let mut buf = vec![0u8; loaded as usize];
        reader.ReadBytes(&mut buf)?;
        let mime = if mime.starts_with("image/") { mime } else { sniff_mime(&buf).to_string() };
        Ok(Some(data_url(&mime, &buf)))
    })
    .flatten()
}

fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(b"RIFF") && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

struct Subscription {
    session: Session,
    tokens: [i64; 3],
}

impl Subscription {
    fn remove(self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.tokens[0]);
        let _ = self.session.RemovePlaybackInfoChanged(self.tokens[1]);
        let _ = self.session.RemoveTimelinePropertiesChanged(self.tokens[2]);
    }
}

fn subscribe(session: &Session, tx: &Sender<Msg>) -> Option<Subscription> {
    let (a, b, c) = (tx.clone(), tx.clone(), tx.clone());
    let t0 = session
        .MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = a.send(Msg::Refresh);
            Ok(())
        }))
        .ok()?;
    let t1 = session
        .PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            let _ = b.send(Msg::Refresh);
            Ok(())
        }))
        .ok()?;
    let t2 = session
        .TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = c.send(Msg::Refresh);
            Ok(())
        }))
        .ok()?;
    Some(Subscription { session: session.clone(), tokens: [t0, t1, t2] })
}

struct Worker {
    tx: Sender<Msg>,
    manager: Manager,
    sessions: Vec<(String, Session)>,
    subs: Vec<Subscription>,
    selected: Option<String>,
    artwork_cache: HashMap<String, Option<String>>,
}

impl Worker {
    fn list_sessions(&self) -> Vec<(String, Session)> {
        let mut out = Vec::new();
        let mut seen: HashMap<String, usize> = HashMap::new();
        if let Ok(view) = self.manager.GetSessions() {
            let n = view.Size().unwrap_or(0);
            for i in 0..n {
                if let Ok(s) = view.GetAt(i) {
                    let aumid = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
                    let count = seen.entry(aumid.clone()).or_insert(0);
                    let id = if *count == 0 { aumid.clone() } else { format!("{aumid}#{count}") };
                    *count += 1;
                    out.push((id, s));
                }
            }
        }
        out
    }

    fn resubscribe(&mut self) {
        let fresh = self.list_sessions();
        let changed = fresh.len() != self.sessions.len()
            || fresh.iter().zip(self.sessions.iter()).any(|(a, b)| a.0 != b.0 || a.1 != b.1);
        if !changed {
            return;
        }
        for sub in self.subs.drain(..) {
            sub.remove();
        }
        self.subs = fresh.iter().filter_map(|(_, s)| subscribe(s, &self.tx)).collect();
        self.sessions = fresh;
        let live: Vec<&String> = self.sessions.iter().map(|(id, _)| id).collect();
        self.artwork_cache.retain(|k, _| live.iter().any(|id| k.starts_with(id.as_str())));
    }

    fn status_of(s: &Session) -> Status {
        s.GetPlaybackInfo().and_then(|p| p.PlaybackStatus()).unwrap_or(Status::Closed)
    }

    fn active(&self) -> Option<(String, Session)> {
        if let Some(sel) = &self.selected {
            if let Some(found) = self.sessions.iter().find(|(id, _)| id == sel) {
                return Some(found.clone());
            }
        }
        if let Ok(current) = self.manager.GetCurrentSession() {
            if let Some(found) = self.sessions.iter().find(|(_, s)| *s == current) {
                return Some(found.clone());
            }
        }
        self.sessions
            .iter()
            .find(|(_, s)| Self::status_of(s) == Status::Playing)
            .or_else(|| self.sessions.first())
            .cloned()
    }

    fn snapshot(&mut self) -> MediaState {
        self.resubscribe();
        if let Some(sel) = &self.selected {
            if !self.sessions.iter().any(|(id, _)| id == sel) {
                self.selected = None;
            }
        }
        let sessions = self
            .sessions
            .iter()
            .map(|(id, s)| SessionInfo {
                id: id.clone(),
                app_name: friendly_app_name(&s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()),
                status: status_str(Self::status_of(s)),
            })
            .collect();
        let active = self.active().map(|(id, s)| self.now_playing(&id, &s));
        MediaState { available: true, sessions, active, manual_selection: self.selected.is_some() }
    }

    fn now_playing(&mut self, id: &str, s: &Session) -> NowPlaying {
        let aumid = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
        let props = read_props(s).unwrap_or(Props { title: String::new(), artist: String::new(), album: String::new() });
        let playback = s.GetPlaybackInfo().ok();
        let status = playback.as_ref().and_then(|p| p.PlaybackStatus().ok()).unwrap_or(Status::Closed);
        let controls = playback.as_ref().and_then(|p| p.Controls().ok());
        let flag = |f: fn(&windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackControls) -> windows::core::Result<bool>| {
            controls.as_ref().and_then(|c| f(c).ok()).unwrap_or(false)
        };
        let can_toggle = flag(|c| c.IsPlayPauseToggleEnabled()) || flag(|c| c.IsPlayEnabled()) || flag(|c| c.IsPauseEnabled());
        let can_position = flag(|c| c.IsPlaybackPositionEnabled());

        let (mut position_ms, mut duration_ms, mut updated_at) = (None, None, None);
        if let Ok(t) = s.GetTimelineProperties() {
            let start = t.StartTime().map(|x| x.Duration).unwrap_or(0);
            let end = t.EndTime().map(|x| x.Duration).unwrap_or(0);
            let pos = t.Position().map(|x| x.Duration).unwrap_or(0);
            let updated = t.LastUpdatedTime().ok().and_then(|d| ticks_to_unix_ms(d.UniversalTime));
            if end > start {
                let dur = (end - start) / 10_000;
                let p = ((pos - start) / 10_000).clamp(0, dur);
                duration_ms = Some(dur);
                position_ms = Some(p);
                updated_at = updated;
            }
        }
        // Only treat position as reliable when the player reports a duration and an update time.
        let reliable = duration_ms.is_some() && updated_at.is_some();

        let key = format!("{id}|{}|{}|{}", props.title, props.artist, props.album);
        let artwork = match self.artwork_cache.get(&key) {
            Some(a) => a.clone(),
            None => {
                let a = if props.title.is_empty() { None } else { read_artwork(s) };
                self.artwork_cache.retain(|k, _| !k.starts_with(&format!("{id}|")));
                self.artwork_cache.insert(key, a.clone());
                a
            }
        };

        NowPlaying {
            session_id: id.to_string(),
            app_name: friendly_app_name(&aumid),
            title: props.title,
            artist: props.artist,
            album: props.album,
            artwork,
            status: status_str(status),
            can_play_pause: can_toggle,
            can_next: flag(|c| c.IsNextEnabled()),
            can_previous: flag(|c| c.IsPreviousEnabled()),
            can_seek: reliable && can_position,
            position_ms: if reliable { position_ms } else { None },
            duration_ms: if reliable { duration_ms } else { None },
            position_updated_at: if reliable { updated_at } else { None },
        }
    }

    fn command(&mut self, cmd: MediaCommand) -> AppResult<()> {
        let (_, s) = self.active().ok_or_else(|| AppError::new("media_none", "No media is playing right now."))?;
        let status = Self::status_of(&s);
        let controls = s.GetPlaybackInfo().and_then(|p| p.Controls()).ok();
        let enabled = |f: fn(&windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackControls) -> windows::core::Result<bool>| {
            controls.as_ref().and_then(|c| f(c).ok()).unwrap_or(false)
        };
        let accepted = match cmd {
            MediaCommand::PlayPause => {
                if enabled(|c| c.IsPlayPauseToggleEnabled()) {
                    with_timeout(move || s.TryTogglePlayPauseAsync()?.join())
                } else if status == Status::Playing {
                    with_timeout(move || s.TryPauseAsync()?.join())
                } else {
                    with_timeout(move || s.TryPlayAsync()?.join())
                }
            }
            MediaCommand::Next => with_timeout(move || s.TrySkipNextAsync()?.join()),
            MediaCommand::Previous => with_timeout(move || s.TrySkipPreviousAsync()?.join()),
        };
        match accepted {
            Some(true) => Ok(()),
            Some(false) => Err(AppError::new("media_unsupported", "This player doesn't support that control.")),
            None => Err(AppError::new("media_timeout", "The media player did not respond.")),
        }
    }

    fn seek(&mut self, position_ms: i64) -> AppResult<()> {
        let (_, s) = self.active().ok_or_else(|| AppError::new("media_none", "No media is playing right now."))?;
        let start = s.GetTimelineProperties().and_then(|t| t.StartTime()).map(|x| x.Duration).unwrap_or(0);
        let target = start + position_ms * 10_000;
        match with_timeout(move || s.TryChangePlaybackPositionAsync(target)?.join()) {
            Some(true) => Ok(()),
            Some(false) => Err(AppError::new("media_unsupported", "This player doesn't support seeking.")),
            None => Err(AppError::new("media_timeout", "The media player did not respond.")),
        }
    }
}

fn run(tx: Sender<Msg>, rx: Receiver<Msg>, state: Arc<RwLock<MediaState>>, emit: Emit) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let publish = |next: MediaState| {
        let mut guard = state.write();
        if *guard != next {
            *guard = next;
            emit(&guard);
        }
    };

    // Acquire the manager; if the service is unavailable keep answering requests gracefully
    // and retry periodically (it can appear after sign-in completes).
    let manager = loop {
        match Manager::RequestAsync().and_then(|op| op.join()) {
            Ok(m) => break m,
            Err(e) => {
                log::warn!("media session manager unavailable: {}", e.message());
                publish(MediaState::default());
                match rx.recv_timeout(Duration::from_secs(60)) {
                    Ok(Msg::Command(_, reply)) | Ok(Msg::Seek(_, reply)) | Ok(Msg::Select(_, reply)) => {
                        let _ = reply.send(Err(unavailable()));
                    }
                    Ok(Msg::Refresh) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        }
    };

    let (a, b) = (tx.clone(), tx.clone());
    let t_sessions = manager
        .SessionsChanged(&TypedEventHandler::new(move |_, _| {
            let _ = a.send(Msg::Refresh);
            Ok(())
        }))
        .ok();
    let t_current = manager
        .CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = b.send(Msg::Refresh);
            Ok(())
        }))
        .ok();

    let mut worker = Worker {
        tx: tx.clone(),
        manager,
        sessions: Vec::new(),
        subs: Vec::new(),
        selected: None,
        artwork_cache: HashMap::new(),
    };
    publish(worker.snapshot());

    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Refresh => {
                // Debounce bursts of events (players often fire several at once).
                let deadline = std::time::Instant::now() + Duration::from_millis(120);
                let mut pending = Vec::new();
                loop {
                    let left = deadline.saturating_duration_since(std::time::Instant::now());
                    match rx.recv_timeout(left) {
                        Ok(Msg::Refresh) => continue,
                        Ok(other) => pending.push(other),
                        Err(_) => break,
                    }
                }
                publish(worker.snapshot());
                for other in pending {
                    let _ = tx.send(other);
                }
            }
            Msg::Command(cmd, reply) => {
                let _ = reply.send(worker.command(cmd));
                let _ = tx.send(Msg::Refresh);
            }
            Msg::Seek(ms, reply) => {
                let _ = reply.send(worker.seek(ms));
                let _ = tx.send(Msg::Refresh);
            }
            Msg::Select(id, reply) => {
                let result = match &id {
                    Some(sel) if !worker.sessions.iter().any(|(sid, _)| sid == sel) => {
                        Err(AppError::not_found("That media session"))
                    }
                    _ => {
                        worker.selected = id;
                        Ok(())
                    }
                };
                let _ = reply.send(result);
                publish(worker.snapshot());
            }
        }
    }

    for sub in worker.subs.drain(..) {
        sub.remove();
    }
    if let Some(t) = t_sessions {
        let _ = worker.manager.RemoveSessionsChanged(t);
    }
    if let Some(t) = t_current {
        let _ = worker.manager.RemoveCurrentSessionChanged(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_names() {
        assert_eq!(friendly_app_name("Spotify.exe"), "Spotify");
        assert_eq!(friendly_app_name("MSEdge"), "Microsoft Edge");
        assert_eq!(friendly_app_name("Chrome"), "Google Chrome");
        assert_eq!(friendly_app_name("308046B0AF4A39CB"), "Firefox");
        assert_eq!(friendly_app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "Media Player");
        assert_eq!(friendly_app_name("someplayer.exe"), "Someplayer");
        assert_eq!(friendly_app_name("Contoso.Player_abc123!App"), "App");
        assert_eq!(friendly_app_name(""), "Media app");
    }

    #[test]
    fn tick_conversion() {
        assert_eq!(ticks_to_unix_ms(0), None);
        assert_eq!(ticks_to_unix_ms(EPOCH_DIFF_TICKS + 10_000), Some(1));
    }

    #[test]
    fn mime_sniffing() {
        assert_eq!(sniff_mime(&[0x89, b'P', b'N', b'G', 0]), "image/png");
        assert_eq!(sniff_mime(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
    }
}
