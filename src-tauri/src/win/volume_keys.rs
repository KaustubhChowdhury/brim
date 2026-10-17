//! Takes over the keyboard volume keys so Windows' own volume pop-up doesn't appear.
//!
//! Windows shows its volume flyout when *it* handles VK_VOLUME_UP/DOWN/MUTE. A low-level
//! keyboard hook on a dedicated thread swallows just those three keys and forwards them to
//! the dock, which changes the volume itself and shows its own pill. Every other key passes
//! straight through; the callback does no work beyond a channel send, so typing is never
//! delayed. Turning the setting off removes the hook entirely.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use parking_lot::Mutex;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKey {
    Up,
    Down,
    Mute,
}

static SINK: OnceLock<Mutex<Option<Sender<VolumeKey>>>> = OnceLock::new();
static THREAD_ID: AtomicU32 = AtomicU32::new(0);

fn sink() -> &'static Mutex<Option<Sender<VolumeKey>>> {
    SINK.get_or_init(|| Mutex::new(None))
}

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let key = match info.vkCode as u16 {
            v if v == VK_VOLUME_UP.0 => Some(VolumeKey::Up),
            v if v == VK_VOLUME_DOWN.0 => Some(VolumeKey::Down),
            v if v == VK_VOLUME_MUTE.0 => Some(VolumeKey::Mute),
            _ => None,
        };
        if let Some(key) = key {
            let down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
            if down {
                if let Some(tx) = sink().try_lock().as_ref().and_then(|g| g.as_ref().cloned()) {
                    let _ = tx.send(key);
                }
            }
            // Swallow both key-down and key-up so Windows never shows its flyout.
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Installs the hook (if not already installed). Keys are delivered to `tx`.
pub fn enable(tx: Sender<VolumeKey>) {
    *sink().lock() = Some(tx);
    if THREAD_ID.load(Ordering::SeqCst) != 0 {
        return;
    }
    std::thread::Builder::new()
        .name("volume-keys".into())
        .spawn(|| unsafe {
            let hhook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), None, 0) {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("volume key hook unavailable: {e}");
                    return;
                }
            };
            THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
            log::info!("volume keys handled by Brim");
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = UnhookWindowsHookEx(hhook);
            THREAD_ID.store(0, Ordering::SeqCst);
            log::info!("volume keys returned to Windows");
        })
        .expect("spawn volume-keys thread");
}

/// Removes the hook; the volume keys go back to Windows.
pub fn disable() {
    *sink().lock() = None;
    let tid = THREAD_ID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}
