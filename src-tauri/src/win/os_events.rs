//! A hidden top-level window on its own thread that turns Windows broadcasts into events.
//!
//! Everything here is event-driven (no polling):
//! * `WM_CLIPBOARDUPDATE` via `AddClipboardFormatListener`
//! * `WM_DISPLAYCHANGE` / work-area `WM_SETTINGCHANGE` for monitor and resolution changes
//! * `WM_POWERBROADCAST` resume and `WM_WTSSESSION_CHANGE` unlock
//! * `ABN_FULLSCREENAPP` via a non-reserving app-bar registration (full-screen detection)
//! * `EVENT_SYSTEM_FOREGROUND` WinEvent hook (focus restore + full-screen re-evaluation)

use super::clipboard::{self, ReadOutcome};
use super::window;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_NEW, ABM_REMOVE, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostQuitMessage,
    PostThreadMessageW, RegisterClassW, RegisterWindowMessageW, TranslateMessage, EVENT_SYSTEM_FOREGROUND, MSG,
    WINDOW_EX_STYLE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_APP, WM_CLIPBOARDUPDATE, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_POWERBROADCAST, WM_QUIT, WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_POPUP,
};

#[derive(Debug, Clone)]
pub enum OsEvent {
    ClipboardText(String),
    ClipboardSkipped(SkipReason),
    DisplayChanged,
    Resumed,
    SessionUnlocked,
    FullscreenMaybeChanged,
    TaskbarCreated,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkipReason {
    TooLarge(usize),
    MarkedPrivate,
    IgnoredApp(String),
}

/// What the clipboard reader needs to know, fetched fresh for every change.
pub struct ClipPolicy {
    pub max_bytes: usize,
    pub ignored_apps: Vec<String>,
}

pub type Sink = Arc<dyn Fn(OsEvent) + Send + Sync>;
pub type PolicyFn = Arc<dyn Fn() -> Option<ClipPolicy> + Send + Sync>;

struct Ctx {
    sink: Sink,
    policy: PolicyFn,
    taskbar_created: u32,
}

static CTX: OnceLock<Ctx> = OnceLock::new();
static HIDDEN_HWND: AtomicIsize = AtomicIsize::new(0);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);

const WM_APPBAR_CALLBACK: u32 = WM_APP + 41;
const ABN_FULLSCREENAPP: usize = 2;
const PBT_APMRESUMESUSPEND: usize = 0x7;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;
const SPI_SETWORKAREA: usize = 0x2F;
const WTS_SESSION_UNLOCK: usize = 0x8;

/// The hidden window handle, used as the clipboard owner for writes.
pub fn hidden_window() -> HWND {
    window::hwnd_from(HIDDEN_HWND.load(Ordering::SeqCst))
}

pub fn start(sink: Sink, policy: PolicyFn) {
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    if CTX.set(Ctx { sink, policy, taskbar_created }).is_err() {
        log::warn!("os event thread already started");
        return;
    }
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("os-events".into())
        .spawn(move || unsafe { run(ready_tx) })
        .expect("spawn os-events thread");
    // Wait (bounded) for the window to exist so clipboard writes have an owner.
    let _ = ready_rx.recv_timeout(std::time::Duration::from_secs(5));
}

pub fn stop() {
    let tid = THREAD_ID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

fn emit(event: OsEvent) {
    if let Some(ctx) = CTX.get() {
        (ctx.sink)(event);
    }
}

unsafe fn register_appbar(hwnd: HWND) {
    let mut abd = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APPBAR_CALLBACK,
        ..Default::default()
    };
    // Registration only — we never call ABM_SETPOS, so no screen space is reserved.
    SHAppBarMessage(ABM_NEW, &mut abd);
}

unsafe fn unregister_appbar(hwnd: HWND) {
    let mut abd =
        APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: hwnd, ..Default::default() };
    SHAppBarMessage(ABM_REMOVE, &mut abd);
}

unsafe extern "system" fn foreground_hook(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if !hwnd.0.is_null() {
        window::note_external_foreground(hwnd);
    }
    emit(OsEvent::FullscreenMaybeChanged);
}

fn handle_clipboard_update(hwnd: HWND) {
    let seq = clipboard::sequence_number();
    if clipboard::is_self_write(seq) {
        return;
    }
    let Some(ctx) = CTX.get() else { return };
    let Some(policy) = (ctx.policy)() else { return };

    // Attribute the copy to a program: the clipboard owner if known, else the foreground window.
    let owner = clipboard::owner_window();
    let source_app = window::process_exe_name(owner).or_else(|| window::process_exe_name(window::foreground()));
    if let Some(app) = &source_app {
        if policy.ignored_apps.iter().any(|a| a == app) {
            emit(OsEvent::ClipboardSkipped(SkipReason::IgnoredApp(app.clone())));
            return;
        }
    }
    match clipboard::read_text(hwnd, policy.max_bytes) {
        ReadOutcome::Text(text) => emit(OsEvent::ClipboardText(text)),
        ReadOutcome::TooLarge(n) => emit(OsEvent::ClipboardSkipped(SkipReason::TooLarge(n))),
        ReadOutcome::MarkedPrivate => emit(OsEvent::ClipboardSkipped(SkipReason::MarkedPrivate)),
        ReadOutcome::NoText => {}
        ReadOutcome::Busy => log::debug!("clipboard busy; change not captured"),
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => {
            // Never let a panic unwind across the FFI boundary.
            let _ = std::panic::catch_unwind(|| handle_clipboard_update(hwnd));
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            emit(OsEvent::DisplayChanged);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETTINGCHANGE => {
            if wparam.0 == SPI_SETWORKAREA {
                emit(OsEvent::DisplayChanged);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_POWERBROADCAST => {
            if wparam.0 == PBT_APMRESUMEAUTOMATIC || wparam.0 == PBT_APMRESUMESUSPEND {
                emit(OsEvent::Resumed);
            }
            LRESULT(1)
        }
        WM_WTSSESSION_CHANGE => {
            if wparam.0 == WTS_SESSION_UNLOCK {
                emit(OsEvent::SessionUnlocked);
            }
            LRESULT(0)
        }
        WM_APPBAR_CALLBACK => {
            if wparam.0 == ABN_FULLSCREENAPP {
                emit(OsEvent::FullscreenMaybeChanged);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => {
            if let Some(ctx) = CTX.get() {
                if ctx.taskbar_created != 0 && msg == ctx.taskbar_created {
                    // Explorer restarted: our app-bar registration was lost.
                    register_appbar(hwnd);
                    emit(OsEvent::TaskbarCreated);
                    return LRESULT(0);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

unsafe fn run(ready: std::sync::mpsc::Sender<()>) {
    THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
    let instance: HINSTANCE = GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).unwrap_or_default();
    let class_name = w!("WispDockOsEvents");
    let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: instance, lpszClassName: class_name, ..Default::default() };
    RegisterClassW(&wc);
    let hwnd = match CreateWindowExW(
        WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0),
        class_name,
        PCWSTR(w!("Brim OS events").as_ptr()),
        WS_POPUP,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(instance),
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            log::error!("could not create OS event window: {e}");
            let _ = ready.send(());
            return;
        }
    };
    HIDDEN_HWND.store(window::raw(hwnd), Ordering::SeqCst);

    if let Err(e) = AddClipboardFormatListener(hwnd) {
        log::error!("clipboard listener unavailable: {e}");
    }
    if let Err(e) = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) {
        log::warn!("session notifications unavailable: {e}");
    }
    register_appbar(hwnd);
    let hook = SetWinEventHook(
        EVENT_SYSTEM_FOREGROUND,
        EVENT_SYSTEM_FOREGROUND,
        None,
        Some(foreground_hook),
        0,
        0,
        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
    );
    // Seed with the current foreground window.
    let fg = window::foreground();
    if window::process_id(fg) != std::process::id() {
        window::note_external_foreground(fg);
    }
    let _ = ready.send(());

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    if !hook.is_invalid() {
        let _ = UnhookWinEvent(hook);
    }
    unregister_appbar(hwnd);
    let _ = RemoveClipboardFormatListener(hwnd);
    let _ = WTSUnRegisterSessionNotification(hwnd);
    let _ = DestroyWindow(hwnd);
    HIDDEN_HWND.store(0, Ordering::SeqCst);
}
