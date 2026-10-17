//! Win32 clipboard access: reading text safely and writing text back.
//!
//! Content that other applications mark as private is never read. Password managers
//! commonly set `ExcludeClipboardContentFromMonitorProcessing`, `Clipboard Viewer Ignore`
//! or `CanIncludeInClipboardHistory = 0`; we honour all three unconditionally.

use crate::error::{AppError, AppResult};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

#[derive(Debug, PartialEq)]
pub enum ReadOutcome {
    Text(String),
    NoText,
    TooLarge(usize),
    MarkedPrivate,
    Busy,
}

struct PrivacyFormats {
    exclude_monitor: u32,
    viewer_ignore: u32,
    can_include_history: u32,
}

fn privacy_formats() -> &'static PrivacyFormats {
    static FORMATS: OnceLock<PrivacyFormats> = OnceLock::new();
    FORMATS.get_or_init(|| unsafe {
        PrivacyFormats {
            exclude_monitor: RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")),
            viewer_ignore: RegisterClipboardFormatW(w!("Clipboard Viewer Ignore")),
            can_include_history: RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")),
        }
    })
}

/// Sequence number of the last clipboard change made by this app (so we don't re-capture it).
static SELF_WRITE_SEQ: AtomicU32 = AtomicU32::new(0);

pub fn sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn is_self_write(seq: u32) -> bool {
    seq != 0 && SELF_WRITE_SEQ.load(Ordering::SeqCst) == seq
}

struct OpenGuard;
impl Drop for OpenGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn open(owner: HWND) -> Option<OpenGuard> {
    // Another application may hold the clipboard briefly; retry a few times.
    for attempt in 0..8 {
        if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
            return Some(OpenGuard);
        }
        std::thread::sleep(Duration::from_millis(10 + attempt * 10));
    }
    None
}

fn format_available(format: u32) -> bool {
    format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

/// HWND of the clipboard owner window, if any (used to attribute the copy to a program).
pub fn owner_window() -> HWND {
    unsafe { GetClipboardOwner() }.unwrap_or_default()
}

/// Reads Unicode text, refusing content marked private or larger than `max_bytes` (UTF-16 bytes).
pub fn read_text(owner: HWND, max_bytes: usize) -> ReadOutcome {
    let fmts = privacy_formats();
    if format_available(fmts.exclude_monitor) || format_available(fmts.viewer_ignore) {
        return ReadOutcome::MarkedPrivate;
    }
    if !format_available(CF_UNICODETEXT.0 as u32) {
        return ReadOutcome::NoText;
    }
    let Some(_guard) = open(owner) else { return ReadOutcome::Busy };
    unsafe {
        if format_available(fmts.can_include_history) {
            if let Ok(h) = GetClipboardData(fmts.can_include_history) {
                let hg = HGLOBAL(h.0);
                let ptr = GlobalLock(hg) as *const u32;
                if !ptr.is_null() {
                    let allowed = GlobalSize(hg) >= 4 && *ptr != 0;
                    let _ = GlobalUnlock(hg);
                    if !allowed {
                        return ReadOutcome::MarkedPrivate;
                    }
                }
            }
        }
        let Ok(handle) = GetClipboardData(CF_UNICODETEXT.0 as u32) else { return ReadOutcome::NoText };
        let hg = HGLOBAL(handle.0);
        let size = GlobalSize(hg);
        if size == 0 {
            return ReadOutcome::NoText;
        }
        if size > max_bytes.saturating_mul(2).saturating_add(2) {
            // UTF-16 can be up to 2x the UTF-8 size for ASCII; final check happens on UTF-8 length.
            return ReadOutcome::TooLarge(size / 2);
        }
        let ptr = GlobalLock(hg) as *const u16;
        if ptr.is_null() {
            return ReadOutcome::NoText;
        }
        let units = std::slice::from_raw_parts(ptr, size / 2);
        let len = units.iter().position(|&c| c == 0).unwrap_or(units.len());
        let text = String::from_utf16_lossy(&units[..len]);
        let _ = GlobalUnlock(hg);
        if text.len() > max_bytes {
            return ReadOutcome::TooLarge(text.len());
        }
        ReadOutcome::Text(text)
    }
}

/// Places `text` on the Windows clipboard. Marks the change as our own write.
pub fn write_text(owner: HWND, text: &str) -> AppResult<()> {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * 2;
    let guard = open(owner).ok_or_else(|| {
        AppError::new("clipboard_busy", "The clipboard is in use by another app. Try again in a moment.")
    })?;
    unsafe {
        EmptyClipboard().map_err(|e| AppError::native("Could not write to the clipboard.", e.message()))?;
        let hmem = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| AppError::native("Out of memory.", e.message()))?;
        let dst = GlobalLock(hmem) as *mut u16;
        if dst.is_null() {
            let _ = GlobalFree(Some(hmem));
            return Err(AppError::native("Could not write to the clipboard.", "GlobalLock failed"));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
        let _ = GlobalUnlock(hmem);
        if let Err(e) = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hmem.0))) {
            // Ownership was not transferred; free it ourselves.
            let _ = GlobalFree(Some(hmem));
            return Err(AppError::native("Could not write to the clipboard.", e.message()));
        }
    }
    drop(guard);
    SELF_WRITE_SEQ.store(sequence_number(), Ordering::SeqCst);
    Ok(())
}
