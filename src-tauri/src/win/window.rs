//! Raw Win32 window operations for the dock window and foreground tracking.
//!
//! The dock window is driven entirely through these functions (not Tauri's window
//! setters) because tao rewrites the extended window style whenever its own flags
//! change, which would drop `WS_EX_TOOLWINDOW` and put the dock back into Alt+Tab.

use super::display::Rect;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, RECT};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, IsZoomed, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    GWL_EXSTYLE, GWL_STYLE, HWND_NOTOPMOST, WS_CAPTION, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER,
    SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

pub fn hwnd_from(raw: isize) -> HWND {
    HWND(raw as *mut core::ffi::c_void)
}

pub fn raw(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

/// Hides the window from the taskbar and Alt+Tab.
pub fn make_tool_window(hwnd: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        if next != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_NOOWNERZORDER,
            );
        }
    }
}

/// Moves and resizes in one atomic call (physical pixels), without activating.
pub fn set_bounds(hwnd: HWND, r: Rect) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            r.left,
            r.top,
            r.width(),
            r.height(),
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

pub fn set_topmost(hwnd: HWND, topmost: bool) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST }),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

pub fn window_rect(hwnd: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r).ok().map(|_| r.into()) }
}

/// Whether the mouse cursor is currently inside the window's rectangle.
pub fn cursor_inside(hwnd: HWND) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    if unsafe { GetCursorPos(&mut p) }.is_err() {
        return false;
    }
    window_rect(hwnd).is_some_and(|r| p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom)
}

pub fn show_no_activate(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

pub fn hide(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}

pub fn is_valid(hwnd: HWND) -> bool {
    !hwnd.0.is_null() && unsafe { IsWindow(Some(hwnd)).as_bool() }
}

/// Brings `hwnd` to the foreground. Succeeds when this process currently holds the
/// foreground right (it was clicked, received a hotkey, or was granted it).
pub fn activate(hwnd: HWND) -> bool {
    unsafe {
        let _ = BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd).as_bool()
    }
}

/// Last foreground window that did not belong to this process (updated by a WinEvent hook).
static LAST_EXTERNAL_FOREGROUND: AtomicIsize = AtomicIsize::new(0);

pub fn note_external_foreground(hwnd: HWND) {
    LAST_EXTERNAL_FOREGROUND.store(raw(hwnd), Ordering::Relaxed);
}

pub fn last_external_foreground() -> Option<HWND> {
    let h = hwnd_from(LAST_EXTERNAL_FOREGROUND.load(Ordering::Relaxed));
    if is_valid(h) { Some(h) } else { None }
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

pub fn process_id(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid
}

/// Lower-cased executable file name (e.g. `keepass.exe`) of the process owning `hwnd`.
pub fn process_exe_name(hwnd: HWND) -> Option<String> {
    if hwnd.0.is_null() {
        return None;
    }
    let pid = process_id(hwnd);
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(handle);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_ascii_lowercase())
    }
}

/// Whether the shell itself reports a full-screen / presentation / D3D exclusive app.
fn shell_reports_fullscreen() -> bool {
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) => state == QUNS_BUSY || state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE,
        Err(_) => false,
    }
}

/// True when the foreground window (not ours) is a full-screen app on the same monitor as
/// `dock`. A window must cover its whole monitor *and* either be borderless and not
/// maximised, or be reported as full-screen by the shell. The second condition matters with
/// an auto-hiding taskbar, where ordinary maximised windows also cover the whole monitor.
pub fn foreground_is_fullscreen_on_monitor_of(dock: HWND) -> Option<String> {
    let fg = foreground();
    if fg.0.is_null() || fg == dock || process_id(fg) == std::process::id() {
        return None;
    }
    if !unsafe { IsWindowVisible(fg).as_bool() } || unsafe { IsIconic(fg).as_bool() } {
        return None;
    }
    let class = class_name(fg);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return None;
    }
    let fg_rect = window_rect(fg)?;
    let mon = super::display::monitor_for_window(fg)?;
    if super::display::monitor_handle_for_window(dock) != mon.handle {
        return None;
    }
    let covers = fg_rect.left <= mon.bounds.left
        && fg_rect.top <= mon.bounds.top
        && fg_rect.right >= mon.bounds.right
        && fg_rect.bottom >= mon.bounds.bottom;
    if !covers {
        return None;
    }
    let style = unsafe { GetWindowLongPtrW(fg, GWL_STYLE) } as u32;
    let has_caption = style & WS_CAPTION.0 == WS_CAPTION.0;
    let maximized = unsafe { IsZoomed(fg).as_bool() };
    if (!maximized && !has_caption) || shell_reports_fullscreen() {
        Some(format!("{} ({class})", process_exe_name(fg).unwrap_or_else(|| "unknown".into())))
    } else {
        None
    }
}
