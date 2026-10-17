//! System volume (Windows Core Audio): read/set master volume and mute, and get notified
//! the instant they change from anywhere — keyboard volume keys, the taskbar, other apps.
//!
//! Runs on its own MTA thread. Everything is event-driven: an `IAudioEndpointVolumeCallback`
//! reports level/mute changes and an `IMMNotificationClient` reports when the default output
//! device changes (e.g. headphones plugged in), at which point we re-attach to the new device.
//! Changes made by this app carry our own event-context GUID so they don't trigger the HUD.

use crate::error::{AppError, AppResult};
use parking_lot::RwLock;
use serde::Serialize;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use windows::core::{implement, GUID, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

/// Identifies volume changes made by Brim itself.
const OUR_CONTEXT: GUID = GUID::from_u128(0x5f2c_9a1e_7b44_4c1d_9e0a_3d6b_8f21_c7a4);

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeState {
    /// False when there is no output device.
    pub available: bool,
    /// 0.0 – 1.0
    pub level: f32,
    pub muted: bool,
}

impl Default for VolumeState {
    fn default() -> Self {
        Self { available: false, level: 0.0, muted: false }
    }
}

enum Msg {
    /// A change reported by Windows. `external` is false when this app made it.
    Changed { level: f32, muted: bool, external: bool },
    DefaultDeviceChanged,
    Set(f32, Sender<AppResult<()>>),
    Mute(bool, Sender<AppResult<()>>),
    /// Volume key: step up/down in Windows' 2% increments.
    Step(i32),
    ToggleMute,
}

/// Called with the new state; `external` is true when the change did not come from this app.
pub type OnChange = Arc<dyn Fn(VolumeState, bool) + Send + Sync>;

#[derive(Clone)]
pub struct VolumeService {
    tx: Sender<Msg>,
    state: Arc<RwLock<VolumeState>>,
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    tx: Sender<Msg>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if let Some(data) = unsafe { pnotify.as_ref() } {
            let _ = self.tx.send(Msg::Changed {
                level: data.fMasterVolume,
                muted: data.bMuted.as_bool(),
                external: data.guidEventContext != OUR_CONTEXT,
            });
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceWatcher {
    tx: Sender<Msg>,
}

impl IMMNotificationClient_Impl for DeviceWatcher_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> {
        let _ = self.tx.send(Msg::DefaultDeviceChanged);
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.tx.send(Msg::DefaultDeviceChanged);
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> windows::core::Result<()> {
        if flow == eRender && (role == eMultimedia || role == eConsole) {
            let _ = self.tx.send(Msg::DefaultDeviceChanged);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

struct Endpoint {
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl Endpoint {
    fn attach(enumerator: &IMMDeviceEnumerator, tx: &Sender<Msg>) -> Option<Endpoint> {
        unsafe {
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
            let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
            let callback: IAudioEndpointVolumeCallback = VolumeCallback { tx: tx.clone() }.into();
            if let Err(e) = volume.RegisterControlChangeNotify(&callback) {
                log::warn!("volume notifications unavailable: {e}");
            }
            Some(Endpoint { volume, callback })
        }
    }

    fn read(&self) -> VolumeState {
        unsafe {
            VolumeState {
                available: true,
                level: self.volume.GetMasterVolumeLevelScalar().unwrap_or(0.0).clamp(0.0, 1.0),
                muted: self.volume.GetMute().map(|m| m.as_bool()).unwrap_or(false),
            }
        }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe {
            let _ = self.volume.UnregisterControlChangeNotify(&self.callback);
        }
    }
}

impl VolumeService {
    pub fn start(on_change: OnChange) -> Self {
        let (tx, rx) = channel();
        let state = Arc::new(RwLock::new(VolumeState::default()));
        let (thread_tx, thread_state) = (tx.clone(), state.clone());
        std::thread::Builder::new()
            .name("volume".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(thread_tx, rx, thread_state, on_change)
                }));
                if result.is_err() {
                    log::error!("volume thread panicked; volume control disabled for this session");
                }
            })
            .expect("spawn volume thread");
        Self { tx, state }
    }

    pub fn state(&self) -> VolumeState {
        *self.state.read()
    }

    fn request(&self, make: impl FnOnce(Sender<AppResult<()>>) -> Msg) -> AppResult<()> {
        let (tx, rx) = channel();
        self.tx.send(make(tx)).map_err(|_| unavailable())?;
        rx.recv_timeout(Duration::from_secs(3)).map_err(|_| unavailable())?
    }

    pub fn set_level(&self, level: f32) -> AppResult<()> {
        if !level.is_finite() {
            return Err(AppError::invalid("Volume must be a number."));
        }
        self.request(|tx| Msg::Set(level.clamp(0.0, 1.0), tx))
    }

    /// Volume keys: one 2% step up (`1`) or down (`-1`). Fire-and-forget.
    pub fn step(&self, direction: i32) {
        let _ = self.tx.send(Msg::Step(direction.signum()));
    }

    pub fn toggle_mute(&self) {
        let _ = self.tx.send(Msg::ToggleMute);
    }

    pub fn set_muted(&self, muted: bool) -> AppResult<()> {
        self.request(|tx| Msg::Mute(muted, tx))
    }
}

fn unavailable() -> AppError {
    AppError::new("volume_unavailable", "No audio output device is available.")
}

fn run(tx: Sender<Msg>, rx: Receiver<Msg>, state: Arc<RwLock<VolumeState>>, on_change: OnChange) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let enumerator: IMMDeviceEnumerator = match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
        Ok(e) => e,
        Err(e) => {
            log::error!("audio device enumerator unavailable: {e}");
            while let Ok(msg) = rx.recv() {
                if let Msg::Set(_, r) | Msg::Mute(_, r) = msg {
                    let _ = r.send(Err(unavailable()));
                }
            }
            return;
        }
    };
    let watcher: IMMNotificationClient = DeviceWatcher { tx: tx.clone() }.into();
    if let Err(e) = unsafe { enumerator.RegisterEndpointNotificationCallback(&watcher) } {
        log::warn!("audio device notifications unavailable: {e}");
    }

    let publish = |next: VolumeState, external: bool| {
        let changed = {
            let mut s = state.write();
            let changed = *s != next;
            *s = next;
            changed
        };
        if changed {
            on_change(next, external);
        }
    };

    let mut endpoint = Endpoint::attach(&enumerator, &tx);
    publish(endpoint.as_ref().map(Endpoint::read).unwrap_or_default(), false);

    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Changed { level, muted, external } => {
                publish(VolumeState { available: true, level: level.clamp(0.0, 1.0), muted }, external);
            }
            Msg::DefaultDeviceChanged => {
                // Device notifications arrive in bursts; let them settle (re-queueing anything
                // else that arrives meanwhile), then re-attach.
                let mut deferred = Vec::new();
                let settle_until = std::time::Instant::now() + Duration::from_millis(200);
                while let Ok(m) = rx.recv_timeout(settle_until.saturating_duration_since(std::time::Instant::now())) {
                    if !matches!(m, Msg::DefaultDeviceChanged) {
                        deferred.push(m);
                    }
                }
                drop(endpoint.take()); // unregister from the old device first
                endpoint = Endpoint::attach(&enumerator, &tx);
                for m in deferred {
                    let _ = tx.send(m);
                }
                publish(endpoint.as_ref().map(Endpoint::read).unwrap_or_default(), false);
            }
            Msg::Set(level, reply) => {
                let r = match &endpoint {
                    Some(ep) => unsafe { ep.volume.SetMasterVolumeLevelScalar(level, &OUR_CONTEXT) }
                        .map_err(|e| AppError::native("Windows did not accept the volume change.", e.message())),
                    None => Err(unavailable()),
                };
                if r.is_ok() {
                    if let Some(ep) = &endpoint {
                        publish(ep.read(), false);
                    }
                }
                let _ = reply.send(r);
            }
            Msg::Step(direction) => {
                if let Some(ep) = &endpoint {
                    let cur = ep.read();
                    // Snap to Windows' 2% grid, like the system volume keys.
                    let next = (((cur.level * 50.0).round() as i32 + direction).clamp(0, 50)) as f32 / 50.0;
                    unsafe {
                        let _ = ep.volume.SetMasterVolumeLevelScalar(next, &OUR_CONTEXT);
                        if cur.muted && direction > 0 {
                            let _ = ep.volume.SetMute(false, &OUR_CONTEXT);
                        }
                    }
                    publish(ep.read(), false);
                }
            }
            Msg::ToggleMute => {
                if let Some(ep) = &endpoint {
                    let cur = ep.read();
                    unsafe {
                        let _ = ep.volume.SetMute(!cur.muted, &OUR_CONTEXT);
                    }
                    publish(ep.read(), false);
                }
            }
            Msg::Mute(muted, reply) => {
                let r = match &endpoint {
                    Some(ep) => unsafe { ep.volume.SetMute(muted, &OUR_CONTEXT) }
                        .map_err(|e| AppError::native("Windows did not accept the mute change.", e.message())),
                    None => Err(unavailable()),
                };
                if r.is_ok() {
                    if let Some(ep) = &endpoint {
                        publish(ep.read(), false);
                    }
                }
                let _ = reply.send(r);
            }
        }
    }
    let _ = unsafe { enumerator.UnregisterEndpointNotificationCallback(&watcher) };
}
