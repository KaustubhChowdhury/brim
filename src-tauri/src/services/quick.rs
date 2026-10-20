//! Quick settings for the Controls tab: Wi-Fi, Bluetooth, the audio output device, Night
//! light and dark mode.
//!
//! Nothing runs in the background: the Controls tab asks for the state every few seconds
//! while it is visible. Each call runs on a short-lived MTA thread with a timeout, so a slow
//! radio or audio driver can never stall the UI.
//!
//! * Wi-Fi and Bluetooth are switched through `Windows.Devices.Radios`, the same API the
//!   Windows quick settings use. The Wi-Fi network name comes from the WLAN API; on builds
//!   where that needs location access, the Network List Manager's name is used instead.
//! * The default output device is changed through `IPolicyConfig`, the long-standing
//!   interface the Windows sound settings use (also used by EarTrumpet and SoundSwitch).
//! * Night light has no public API. Its state lives in a small CloudStore blob; we only
//!   touch it when the blob has the exact known layout, and otherwise report it as
//!   unavailable so the tile falls back to opening Settings.
//! * Dark mode is the documented `Personalize` registry pair plus the broadcast Windows
//!   itself sends, so open apps switch immediately.

use crate::error::{AppError, AppResult};
use crate::win::shell;
use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::sync::mpsc::channel;
use std::time::Duration;
use windows::core::{w, GUID, HSTRING, PCWSTR};
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{HANDLE, LPARAM, WPARAM};
use windows::Win32::Media::Audio::{
    eCommunications, eConsole, eMultimedia, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::NetworkManagement::WiFi::{
    wlan_interface_state_connected, wlan_intf_opcode_current_connection, WlanCloseHandle, WlanEnumInterfaces,
    WlanFreeMemory, WlanOpenHandle, WlanQueryInterface, WLAN_CONNECTION_ATTRIBUTES, WLAN_INTERFACE_INFO,
    WLAN_INTERFACE_INFO_LIST,
};
use windows::Win32::Networking::NetworkListManager::{INetworkListManager, NetworkListManager, NLM_ENUM_NETWORK_CONNECTED};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToBSTR};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};
use windows::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_BINARY, REG_DWORD, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD,
};
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioTile {
    pub on: bool,
    pub connected: bool,
    /// The Wi-Fi network name, when connected and Windows shares it.
    pub network: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioOutput {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickState {
    /// None when the PC has no Wi-Fi (or Bluetooth) radio.
    pub wifi: Option<RadioTile>,
    pub bluetooth: Option<RadioTile>,
    /// None when Night light's state can't be read on this PC.
    pub night_light: Option<bool>,
    /// False until Night light has been switched on once in Windows (until then its stored
    /// state is a short placeholder Brim can't safely extend), or on an unknown layout.
    pub night_light_switchable: bool,
    pub dark_mode: bool,
    pub outputs: Vec<AudioOutput>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum QuickSetting {
    Wifi,
    Bluetooth,
    NightLight,
    DarkMode,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum QuickPage {
    Wifi,
    Bluetooth,
    Sound,
    NightLight,
    Colors,
}

impl QuickPage {
    fn uri(self) -> &'static str {
        match self {
            QuickPage::Wifi => "ms-settings:network-wifi",
            QuickPage::Bluetooth => "ms-settings:bluetooth",
            QuickPage::Sound => "ms-settings:sound",
            QuickPage::NightLight => "ms-settings:nightlight",
            QuickPage::Colors => "ms-settings:colors",
        }
    }
}

/// Runs `f` on a fresh MTA thread and waits at most `CALL_TIMEOUT`.
fn on_mta<T: Send + 'static>(what: &'static str, f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("quick-settings".into())
        .spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let _ = tx.send(f());
        })
        .map_err(|e| AppError::internal(e.to_string()))?;
    rx.recv_timeout(CALL_TIMEOUT).unwrap_or_else(|_| {
        log::warn!("quick settings: {what} timed out");
        Err(AppError::new("quick_timeout", "Windows took too long to respond. Try again."))
    })
}

pub fn state() -> AppResult<QuickState> {
    on_mta("state", || Ok(read_state()))
}

fn read_state() -> QuickState {
    let radios = radios().unwrap_or_default();
    let wifi = radio_state(&radios, RadioKind::WiFi).map(|on| {
        let link = if on { wifi_link() } else { None };
        RadioTile {
            on,
            connected: link.is_some(),
            network: link.and_then(|l| l.ssid.or_else(|| network_name(l.interface))),
        }
    });
    let bluetooth = radio_state(&radios, RadioKind::Bluetooth).map(|on| RadioTile { on, connected: false, network: None });
    let night_light = read_binary(NIGHT_LIGHT_KEY, w!("Data"));
    QuickState {
        wifi,
        bluetooth,
        night_light: night_light.as_deref().and_then(night_light_shown),
        night_light_switchable: night_light.as_deref().and_then(night_light_on).is_some(),
        dark_mode: read_dword(PERSONALIZE_KEY, w!("AppsUseLightTheme")).map(|v| v == 0).unwrap_or(false),
        outputs: audio_outputs().unwrap_or_else(|e| {
            log::debug!("audio outputs unavailable: {}", e.message());
            Vec::new()
        }),
    }
}

pub fn set(setting: QuickSetting, on: bool) -> AppResult<QuickState> {
    on_mta("set", move || {
        match setting {
            QuickSetting::Wifi => set_radio(RadioKind::WiFi, on)?,
            QuickSetting::Bluetooth => set_radio(RadioKind::Bluetooth, on)?,
            QuickSetting::NightLight => set_night_light(on)?,
            QuickSetting::DarkMode => set_dark_mode(on)?,
        }
        Ok(read_state())
    })
}

pub fn set_output(id: String) -> AppResult<QuickState> {
    on_mta("set_output", move || {
        // Only ever switch to a device Windows itself listed as an active output.
        let outputs = audio_outputs().map_err(|e| AppError::native("Couldn't list audio devices.", e.message()))?;
        if !outputs.iter().any(|o| o.id == id) {
            return Err(AppError::not_found("audio device"));
        }
        unsafe {
            let config: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)
                .map_err(|e| AppError::native("Windows didn't let Brim change the output device.", e.message()))?;
            let wide = HSTRING::from(id.as_str());
            for role in [eConsole, eMultimedia, eCommunications] {
                policy::set_default_endpoint(&config, PCWSTR(wide.as_ptr()), role.0 as u32)
                    .ok()
                    .map_err(|e| AppError::native("Windows didn't accept the output device.", e.message()))?;
            }
        }
        Ok(read_state())
    })
}

pub fn open(page: QuickPage) -> AppResult<()> {
    shell::shell_open(page.uri(), None)
}

// ---------------- Radios ----------------

fn radios() -> windows::core::Result<Vec<Radio>> {
    Ok(Radio::GetRadiosAsync()?.join()?.into_iter().collect())
}

fn find_radio(radios: &[Radio], kind: RadioKind) -> Option<&Radio> {
    radios.iter().find(|r| r.Kind().ok() == Some(kind))
}

fn radio_state(radios: &[Radio], kind: RadioKind) -> Option<bool> {
    find_radio(radios, kind).map(|r| r.State().ok() == Some(RadioState::On))
}

fn set_radio(kind: RadioKind, on: bool) -> AppResult<()> {
    let name = if kind == RadioKind::WiFi { "Wi-Fi" } else { "Bluetooth" };
    let fail = |e: windows::core::Error| AppError::native(format!("Couldn't change {name}."), e.message());
    let access = Radio::RequestAccessAsync().and_then(|op| op.join()).map_err(fail)?;
    if access != RadioAccessStatus::Allowed {
        return Err(AppError::new("radio_denied", format!("Windows didn't allow Brim to change {name}.")));
    }
    let radios = radios().map_err(fail)?;
    let radio = find_radio(&radios, kind).ok_or_else(|| AppError::new("radio_missing", format!("This PC has no {name} radio.")))?;
    if radio.State().ok() == Some(RadioState::Disabled) {
        return Err(AppError::new("radio_disabled", format!("{name} is switched off in hardware or by airplane mode.")));
    }
    let target = if on { RadioState::On } else { RadioState::Off };
    let result = radio.SetStateAsync(target).and_then(|op| op.join()).map_err(fail)?;
    if result != RadioAccessStatus::Allowed {
        return Err(AppError::new("radio_denied", format!("Windows didn't allow Brim to change {name}.")));
    }
    Ok(())
}

// ---------------- Wi-Fi network ----------------

struct WifiLink {
    interface: GUID,
    ssid: Option<String>,
}

/// The connected Wi-Fi interface and, when Windows shares it, the network name.
fn wifi_link() -> Option<WifiLink> {
    unsafe {
        let (mut version, mut handle) = (0u32, HANDLE::default());
        if WlanOpenHandle(2, None, &mut version, &mut handle) != 0 {
            return None;
        }
        let mut found = None;
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(handle, None, &mut list) == 0 && !list.is_null() {
            let count = (*list).dwNumberOfItems as usize;
            let first = std::ptr::addr_of!((*list).InterfaceInfo) as *const WLAN_INTERFACE_INFO;
            for info in std::slice::from_raw_parts(first, count) {
                if info.isState != wlan_interface_state_connected {
                    continue;
                }
                let (mut size, mut data) = (0u32, std::ptr::null_mut::<c_void>());
                let ssid = if WlanQueryInterface(
                    handle,
                    &info.InterfaceGuid,
                    wlan_intf_opcode_current_connection,
                    None,
                    &mut size,
                    &mut data,
                    None,
                ) == 0
                    && !data.is_null()
                {
                    let attrs = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
                    let raw = &attrs.wlanAssociationAttributes.dot11Ssid;
                    let len = (raw.uSSIDLength as usize).min(raw.ucSSID.len());
                    let name = String::from_utf8_lossy(&raw.ucSSID[..len]).trim().to_string();
                    WlanFreeMemory(data);
                    (!name.is_empty()).then_some(name)
                } else {
                    None
                };
                found = Some(WifiLink { interface: info.InterfaceGuid, ssid });
                break;
            }
            WlanFreeMemory(list as *const c_void);
        }
        let _ = WlanCloseHandle(handle, None);
        found
    }
}

/// The Network List Manager's name for the network on `adapter` (no location access needed).
fn network_name(adapter: GUID) -> Option<String> {
    unsafe {
        let nlm: INetworkListManager = CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok()?;
        let networks = nlm.GetNetworks(NLM_ENUM_NETWORK_CONNECTED).ok()?;
        loop {
            let (mut slot, mut fetched) = ([None], 0u32);
            if networks.Next(&mut slot, Some(&mut fetched)).is_err() || fetched == 0 {
                return None;
            }
            let Some(network) = slot[0].take() else { return None };
            let Ok(connections) = network.GetNetworkConnections() else { continue };
            loop {
                let (mut slot, mut fetched) = ([None], 0u32);
                if connections.Next(&mut slot, Some(&mut fetched)).is_err() || fetched == 0 {
                    break;
                }
                if slot[0].take().and_then(|c| c.GetAdapterId().ok()) == Some(adapter) {
                    return network.GetName().ok().map(|n| n.to_string()).filter(|n| !n.is_empty());
                }
            }
        }
    }
}

// ---------------- Audio output ----------------

/// The sound settings' policy interface. Method names and order must match its vtable exactly.
#[allow(non_snake_case)]
mod policy {
    use std::ffi::c_void;
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, HRESULT, PCWSTR};

    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub unsafe trait IPolicyConfig: IUnknown {
        fn GetMixFormat(&self, id: PCWSTR, format: *mut *mut c_void) -> HRESULT;
        fn GetDeviceFormat(&self, id: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
        fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
        fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
        fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, default_period: *mut i64, min_period: *mut i64) -> HRESULT;
        fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
        fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
        fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
        fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
        fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
        fn SetDefaultEndpoint(&self, id: PCWSTR, role: u32) -> HRESULT;
        fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
    }

    /// Makes `id` the default output for `role`.
    pub unsafe fn set_default_endpoint(config: &IPolicyConfig, id: PCWSTR, role: u32) -> HRESULT {
        unsafe { config.SetDefaultEndpoint(id, role) }
    }
}
use policy::IPolicyConfig;

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

unsafe fn device_id(device: &IMMDevice) -> windows::core::Result<String> {
    let raw = device.GetId()?;
    let id = raw.to_string().unwrap_or_default();
    CoTaskMemFree(Some(raw.0 as *const c_void));
    Ok(id)
}

unsafe fn device_name(device: &IMMDevice) -> Option<String> {
    let store = device.OpenPropertyStore(STGM_READ).ok()?;
    let mut value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
    let name = PropVariantToBSTR(&value).ok().map(|b| b.to_string());
    let _ = PropVariantClear(&mut value);
    name.filter(|n| !n.is_empty())
}

fn audio_outputs() -> windows::core::Result<Vec<AudioOutput>> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let default_id = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).and_then(|d| device_id(&d)).ok();
        let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut out = Vec::new();
        for i in 0..devices.GetCount()? {
            let device = devices.Item(i)?;
            let id = device_id(&device)?;
            out.push(AudioOutput {
                is_default: default_id.as_deref() == Some(id.as_str()),
                name: device_name(&device).unwrap_or_else(|| "Audio device".into()),
                id,
            });
        }
        Ok(out)
    }
}

// ---------------- Registry ----------------

const PERSONALIZE_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
const NIGHT_LIGHT_KEY: PCWSTR = w!(
    r"Software\Microsoft\Windows\CurrentVersion\CloudStore\Store\DefaultAccount\Current\default$windows.data.bluelightreduction.bluelightreductionstate\windows.data.bluelightreduction.bluelightreductionstate"
);

fn read_dword(key: PCWSTR, value: PCWSTR) -> Option<u32> {
    let (mut data, mut size) = (0u32, 4u32);
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key, value, RRF_RT_REG_DWORD, None, Some(&mut data as *mut u32 as *mut c_void), Some(&mut size))
    };
    r.is_ok().then_some(data)
}

fn write_dword(key: PCWSTR, value: PCWSTR, data: u32) -> AppResult<()> {
    let bytes = data.to_le_bytes();
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, key, value, REG_DWORD.0, Some(bytes.as_ptr() as *const c_void), 4) }
        .ok()
        .map_err(|e| AppError::native("Couldn't save the setting.", e.message()))
}

fn read_binary(key: PCWSTR, value: PCWSTR) -> Option<Vec<u8>> {
    let mut size = 0u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, key, value, RRF_RT_REG_BINARY, None, None, Some(&mut size)) }.ok().ok()?;
    let mut buf = vec![0u8; size as usize];
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key, value, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as *mut c_void), Some(&mut size))
    }
    .ok()
    .ok()?;
    buf.truncate(size as usize);
    Some(buf)
}

// ---------------- Night light ----------------

/// Byte 18 is the length of the nested state record: 0x15 when on (it gains a two-byte
/// `10 00` field at offset 23), 0x13 when off. Anything else is a layout we don't know.
const NL_LEN_AT: usize = 18;
const NL_FIELD_AT: usize = 23;
const NL_ON: u8 = 0x15;
const NL_OFF: u8 = 0x13;
const NL_ON_FIELD: [u8; 2] = [0x10, 0x00];
const NL_RECORD_HEADER: [u8; 4] = [0x43, 0x42, 0x01, 0x00];

pub fn night_light_on(data: &[u8]) -> Option<bool> {
    if data.len() < NL_FIELD_AT + 2 || data[NL_LEN_AT + 1..NL_FIELD_AT] != NL_RECORD_HEADER {
        return None;
    }
    match data[NL_LEN_AT] {
        NL_ON if data[NL_FIELD_AT..NL_FIELD_AT + 2] == NL_ON_FIELD => Some(true),
        NL_OFF => Some(false),
        _ => None,
    }
}

/// The state to show: also "off" for the short placeholder Windows keeps until Night light
/// is first turned on.
pub fn night_light_shown(data: &[u8]) -> Option<bool> {
    night_light_on(data).or_else(|| (data.len() <= NL_LEN_AT && data.starts_with(&NL_RECORD_HEADER)).then_some(false))
}

/// The blob with Night light switched `on`, or None if the layout isn't the known one.
pub fn night_light_blob(data: &[u8], on: bool) -> Option<Vec<u8>> {
    let current = night_light_on(data)?;
    if current == on {
        return Some(data.to_vec());
    }
    let mut next = Vec::with_capacity(data.len() + 2);
    next.extend_from_slice(&data[..NL_FIELD_AT]);
    if on {
        next.extend_from_slice(&NL_ON_FIELD);
        next.extend_from_slice(&data[NL_FIELD_AT..]);
    } else {
        next.extend_from_slice(&data[NL_FIELD_AT + 2..]);
    }
    next[NL_LEN_AT] = if on { NL_ON } else { NL_OFF };
    // Bump the record's timestamp so Windows notices the change.
    if let Some(b) = next[10..15].iter_mut().find(|b| **b != 0xff) {
        *b += 1;
    }
    Some(next)
}

fn set_night_light(on: bool) -> AppResult<()> {
    let unavailable = || AppError::new("night_light_unavailable", "Night light can't be switched from Brim on this PC. Open its settings instead.");
    let data = read_binary(NIGHT_LIGHT_KEY, w!("Data")).ok_or_else(unavailable)?;
    let next = night_light_blob(&data, on).ok_or_else(unavailable)?;
    if next == data {
        return Ok(());
    }
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, NIGHT_LIGHT_KEY, w!("Data"), REG_BINARY.0, Some(next.as_ptr() as *const c_void), next.len() as u32)
    }
    .ok()
    .map_err(|e| AppError::native("Couldn't change Night light.", e.message()))
}

// ---------------- Dark mode ----------------

fn set_dark_mode(dark: bool) -> AppResult<()> {
    let light = u32::from(!dark);
    write_dword(PERSONALIZE_KEY, w!("AppsUseLightTheme"), light)?;
    write_dword(PERSONALIZE_KEY, w!("SystemUsesLightTheme"), light)?;
    // The same notification Settings sends, so open apps and the taskbar switch right away.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            1000,
            None,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A full "off" blob in the layout Windows writes (timestamp bytes 10..15 vary).
    const OFF: [u8; 41] = [
        0x43, 0x42, 0x01, 0x00, 0x0A, 0x02, 0x01, 0x00, 0x2A, 0x06, 0x9E, 0xC2, 0xBB, 0xC6, 0x06, 0x2A, 0x2B, 0x0E,
        0x13, 0x43, 0x42, 0x01, 0x00, 0xD0, 0x0A, 0x02, 0xC6, 0x14, 0xB8, 0x8E, 0x9D, 0xD0, 0xB4, 0xC0, 0xAE, 0xE9,
        0x01, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn reads_and_toggles_night_light() {
        assert_eq!(night_light_on(&OFF), Some(false));
        let on = night_light_blob(&OFF, true).unwrap();
        assert_eq!(on.len(), OFF.len() + 2);
        assert_eq!(night_light_on(&on), Some(true));
        assert_eq!(on[10], OFF[10] + 1);
        let off = night_light_blob(&on, false).unwrap();
        assert_eq!(night_light_on(&off), Some(false));
        assert_eq!(off[NL_FIELD_AT..], OFF[NL_FIELD_AT..]);
        // Already in the requested state: unchanged.
        assert_eq!(night_light_blob(&OFF, false).unwrap(), OFF.to_vec());
    }

    #[test]
    fn refuses_unknown_night_light_layouts() {
        assert_eq!(night_light_on(&[]), None);
        let mut odd = OFF;
        odd[NL_LEN_AT] = 0x14;
        assert_eq!(night_light_on(&odd), None);
        assert!(night_light_blob(&odd, true).is_none());
        let mut bad_header = OFF;
        bad_header[20] = 0x00;
        assert!(night_light_blob(&bad_header, true).is_none());
    }

    /// Reads (never changes) this PC's real quick settings: `cargo test live_quick_state -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_quick_state() {
        println!("{:#?}", state().unwrap());
    }

    #[test]
    fn placeholder_reads_as_off_but_is_not_switchable() {
        // What Windows stores before Night light has ever been turned on.
        let placeholder = [0x43, 0x42, 0x01, 0x00, 0x0A, 0x02, 0x01, 0x00, 0x2A, 0x2A, 0x00, 0x00, 0x00];
        assert_eq!(night_light_shown(&placeholder), Some(false));
        assert_eq!(night_light_on(&placeholder), None);
        assert!(night_light_blob(&placeholder, true).is_none());
        assert_eq!(night_light_shown(&OFF), Some(false));
    }
}
