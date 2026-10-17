//! Monitor enumeration and geometry helpers (physical pixels, per-monitor DPI aware).

use serde::{Deserialize, Serialize};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, DISPLAY_DEVICEW, HDC,
    HMONITOR, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{EDD_GET_DEVICE_INTERFACE_NAME, MONITORINFOF_PRIMARY};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self { left, top, right: left + width, bottom: top + height }
    }
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    pub fn center_x(&self) -> i32 {
        self.left + self.width() / 2
    }
    pub fn center_y(&self) -> i32 {
        self.top + self.height() / 2
    }
    #[cfg(test)]
    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.left >= self.left && other.top >= self.top && other.right <= self.right && other.bottom <= self.bottom
    }
    #[cfg(test)]
    pub fn intersection_area(&self, other: &Rect) -> i64 {
        let w = (self.right.min(other.right) - self.left.max(other.left)).max(0) as i64;
        let h = (self.bottom.min(other.bottom) - self.top.max(other.top)).max(0) as i64;
        w * h
    }
    /// Moves `self` (keeping its size) so it lies inside `bounds` where possible.
    pub fn clamped_into(&self, bounds: &Rect) -> Rect {
        let w = self.width().min(bounds.width());
        let h = self.height().min(bounds.height());
        let left = self.left.clamp(bounds.left, bounds.right - w);
        let top = self.top.clamp(bounds.top, bounds.bottom - h);
        Rect::new(left, top, w, h)
    }
}

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Self { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// Stable-ish identifier: the device interface path when available, else the GDI device name.
    pub id: String,
    pub handle: isize,
    pub bounds: Rect,
    pub work: Rect,
    pub primary: bool,
    pub scale: f64,
}

unsafe extern "system" fn collect_monitor(hmon: HMONITOR, _hdc: HDC, _rect: *mut RECT, data: LPARAM) -> BOOL {
    let list = &mut *(data.0 as *mut Vec<HMONITOR>);
    list.push(hmon);
    BOOL(1)
}

fn describe(hmon: HMONITOR) -> Option<Monitor> {
    unsafe {
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if !GetMonitorInfoW(hmon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
            return None;
        }
        let device_len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
        let device_name = String::from_utf16_lossy(&info.szDevice[..device_len]);

        let mut dd = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        let id = if EnumDisplayDevicesW(
            windows::core::PCWSTR(info.szDevice.as_ptr()),
            0,
            &mut dd,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
        .as_bool()
        {
            let len = dd.DeviceID.iter().position(|&c| c == 0).unwrap_or(dd.DeviceID.len());
            let s = String::from_utf16_lossy(&dd.DeviceID[..len]);
            if s.is_empty() { device_name.clone() } else { s }
        } else {
            device_name.clone()
        };

        let (mut dx, mut dy) = (96u32, 96u32);
        let scale = if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() && dx > 0 {
            dx as f64 / 96.0
        } else {
            1.0
        };
        Some(Monitor {
            id,
            handle: hmon.0 as isize,
            bounds: info.monitorInfo.rcMonitor.into(),
            work: info.monitorInfo.rcWork.into(),
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            scale,
        })
    }
}

pub fn monitors() -> Vec<Monitor> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(&mut handles as *mut _ as isize));
    }
    handles.into_iter().filter_map(describe).collect()
}

pub fn primary_monitor() -> Option<Monitor> {
    let mons = monitors();
    mons.iter().find(|m| m.primary).cloned().or_else(|| mons.into_iter().next()).or_else(|| unsafe {
        describe(MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY))
    })
}

pub fn monitor_for_window(hwnd: HWND) -> Option<Monitor> {
    unsafe { describe(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)) }
}

pub fn monitor_handle_for_window(hwnd: HWND) -> isize {
    unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST).0 as isize }
}

pub fn monitor_for_point(x: i32, y: i32) -> Option<Monitor> {
    unsafe { describe(MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_rect_inside_bounds() {
        let bounds = Rect::new(0, 0, 1000, 800);
        assert_eq!(Rect::new(-50, -10, 200, 50).clamped_into(&bounds), Rect::new(0, 0, 200, 50));
        assert_eq!(Rect::new(950, 790, 200, 50).clamped_into(&bounds), Rect::new(800, 750, 200, 50));
        // Larger than bounds: shrinks to fit.
        assert_eq!(Rect::new(10, 10, 2000, 50).clamped_into(&bounds), Rect::new(0, 10, 1000, 50));
    }

    #[test]
    fn intersection_area() {
        let a = Rect::new(0, 0, 100, 100);
        assert_eq!(a.intersection_area(&Rect::new(50, 50, 100, 100)), 2500);
        assert_eq!(a.intersection_area(&Rect::new(200, 200, 10, 10)), 0);
    }
}
