//! System stats (CPU, memory, disk, battery, uptime) and power actions.
//!
//! Stats are read on demand (the Stats panel asks about once a second while it is visible),
//! so nothing runs in the background. CPU usage is the busy share of the time elapsed since
//! the previous sample.

use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SetSuspendState, SYSTEM_POWER_STATUS};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::SystemInformation::{GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    pub plugged_in: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStats {
    /// 0–100, or None on the very first sample.
    pub cpu_percent: Option<f32>,
    pub memory_used: u64,
    pub memory_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub disk_label: String,
    pub battery: Option<Battery>,
    pub uptime_secs: u64,
}

/// Who and what this PC is, for the dashboard. Read once when the dashboard opens.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    /// The account's full name as Windows shows it, else the sign-in name.
    pub user_name: String,
    /// "Windows 11" or "Windows 10".
    pub os_name: String,
    /// Feature update, e.g. "25H2" (empty if unknown).
    pub os_version: String,
    pub computer_name: String,
}

pub fn info() -> SystemInfo {
    use windows::core::w;
    let login = std::env::var("USERNAME").unwrap_or_default();
    let build = reg_string(w!("CurrentBuild")).and_then(|b| b.parse::<u32>().ok()).unwrap_or(0);
    SystemInfo {
        user_name: full_name(&login).unwrap_or(login),
        // ProductName still says "Windows 10" on Windows 11; the build number is what tells them apart.
        os_name: if build >= 22000 { "Windows 11" } else { "Windows 10" }.into(),
        os_version: reg_string(w!("DisplayVersion")).unwrap_or_default(),
        computer_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
    }
}

/// A REG_SZ value under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`.
fn reg_string(value: windows::core::PCWSTR) -> Option<String> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let key = windows::core::w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let mut buf = [0u16; 128];
    let mut size = std::mem::size_of_val(&buf) as u32;
    unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, key, value, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size)) }
        .ok()
        .ok()?;
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len])).filter(|s| !s.is_empty())
}

/// The local account's full name (for Microsoft accounts, the name on the account).
fn full_name(login: &str) -> Option<String> {
    use windows::Win32::NetworkManagement::NetManagement::{NetApiBufferFree, NetUserGetInfo, USER_INFO_10};
    if login.is_empty() {
        return None;
    }
    let user = windows::core::HSTRING::from(login);
    let mut buf: *mut u8 = std::ptr::null_mut();
    unsafe {
        if NetUserGetInfo(windows::core::PCWSTR::null(), &user, 10, &mut buf) != 0 || buf.is_null() {
            return None;
        }
        let info = &*(buf as *const USER_INFO_10);
        let name = info.usri10_full_name.to_string().ok().map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        NetApiBufferFree(Some(buf as *const std::ffi::c_void));
        name
    }
}

fn ft(t: FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
}

/// (idle, kernel+user) from the previous sample.
static LAST_CPU: Mutex<Option<(u64, u64)>> = Mutex::new(None);

fn cpu_percent() -> Option<f32> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.ok()?;
    // Kernel time includes idle time.
    let (idle, total) = (ft(idle), ft(kernel) + ft(user));
    let mut last = LAST_CPU.lock();
    let result = last.and_then(|(pi, pt)| {
        let (di, dt) = (idle.saturating_sub(pi), total.saturating_sub(pt));
        (dt > 0).then(|| (100.0 * (1.0 - di as f64 / dt as f64)).clamp(0.0, 100.0) as f32)
    });
    *last = Some((idle, total));
    result
}

pub fn stats() -> SystemStats {
    let mut mem = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let (memory_used, memory_total) = if unsafe { GlobalMemoryStatusEx(&mut mem) }.is_ok() {
        (mem.ullTotalPhys - mem.ullAvailPhys, mem.ullTotalPhys)
    } else {
        (0, 0)
    };

    // The drive Windows is installed on.
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let root = windows::core::HSTRING::from(format!("{system_drive}\\"));
    let (mut free, mut total) = (0u64, 0u64);
    let (disk_used, disk_total) =
        if unsafe { GetDiskFreeSpaceExW(&root, None, Some(&mut total), Some(&mut free)) }.is_ok() {
            (total - free, total)
        } else {
            (0, 0)
        };

    let mut power = SYSTEM_POWER_STATUS::default();
    let battery = if unsafe { GetSystemPowerStatus(&mut power) }.is_ok()
        && power.BatteryFlag != 128 // no system battery
        && power.BatteryFlag != 255 // unknown
        && power.BatteryLifePercent <= 100
    {
        Some(Battery {
            percent: power.BatteryLifePercent,
            charging: power.BatteryFlag & 8 != 0,
            plugged_in: power.ACLineStatus == 1,
        })
    } else {
        None
    };

    SystemStats {
        cpu_percent: cpu_percent(),
        memory_used,
        memory_total,
        disk_used,
        disk_total,
        disk_label: system_drive,
        battery,
        uptime_secs: unsafe { GetTickCount64() } / 1000,
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PowerAction {
    Lock,
    Sleep,
    Restart,
    Shutdown,
    Signout,
}

/// Performs a power action. Restart, shut down and sign out use Windows' own `shutdown.exe`
/// with fixed arguments, so other apps get the normal chance to save their work.
pub fn power(action: PowerAction) -> AppResult<()> {
    let fail = |e: String| AppError::native("Windows did not allow that power action.", e);
    match action {
        PowerAction::Lock => unsafe { LockWorkStation() }.map_err(|e| fail(e.message())),
        PowerAction::Sleep => {
            // hibernate = false, force = false, disable wake events = false
            if unsafe { SetSuspendState(false, false, false) } { Ok(()) } else { Err(fail("SetSuspendState failed".into())) }
        }
        PowerAction::Restart | PowerAction::Shutdown | PowerAction::Signout => {
            let args: &[&str] = match action {
                PowerAction::Restart => &["/r", "/t", "0"],
                PowerAction::Shutdown => &["/s", "/t", "0"],
                _ => &["/l"],
            };
            let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
            let exe = std::path::Path::new(&windir).join("System32").join("shutdown.exe");
            let status = std::process::Command::new(exe).args(args).status().map_err(|e| fail(e.to_string()))?;
            if status.success() { Ok(()) } else { Err(fail(format!("shutdown.exe exited with {status}"))) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_are_sane() {
        let first = stats();
        assert!(first.memory_total > 0 && first.memory_used <= first.memory_total);
        assert!(first.disk_total > 0 && first.disk_used <= first.disk_total);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let second = stats();
        let cpu = second.cpu_percent.expect("second sample has a CPU reading");
        assert!((0.0..=100.0).contains(&cpu));
    }

    #[test]
    fn power_actions_parse() {
        let a: PowerAction = serde_json::from_str("\"signout\"").unwrap();
        assert_eq!(a, PowerAction::Signout);
        assert!(serde_json::from_str::<PowerAction>("\"format\"").is_err());
    }
}

#[cfg(test)]
mod info_tests {
    /// Reads this PC's name and Windows version: `cargo test live_system_info -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_system_info() {
        println!("{:#?}", super::info());
    }
}
