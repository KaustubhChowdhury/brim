# Brim

A local-first floating utility dock for Windows 10 and 11. When idle it vanishes, or folds into a thin line at the top of the screen (your choice). Hover where it was and it grows back into the island; click the island or press **Alt+Shift+Space** to open:

| Utility | What it does |
|---|---|
| Music | Now playing, play/pause, skip and seek, for any app with Windows media controls |
| Clipboard | Searchable text history with pin, reuse, delete, retention, pause and privacy filters |
| Apps | Start-menu app search, pinned and recent apps, custom programs and shortcuts |
| Tasks | A Today view, tasks with due dates and priorities, and reminders with Windows notifications |
| Calendar | A local month calendar with events |
| Snippets | Reusable text with categories and one-click copy |
| Wallpapers | A local gallery with preview, fit modes and "set as wallpaper" |
| Stats | Live CPU, memory, disk, battery and uptime |
| Power | Lock, sleep, restart, shut down, sign out |

All four dock slots are configurable, and can also launch any app directly. No account, no cloud, no AI, no telemetry.

Built from `Product Requirements Document.md`. Inspired by the publicly described feature set of Hitodama; this is an independent implementation, not affiliated with it.

## Requirements (development)

- Node.js 20+
- Rust (stable, MSVC toolchain)
- Visual Studio Build Tools with the "Desktop development with C++" workload
- WebView2 runtime (preinstalled on Windows 11)

On this machine the Rust and MSVC toolchains are installed under `E:\DevTools`; `CARGO_HOME` and `RUSTUP_HOME` point there.

## Commands

```bash
npm install
npm run app:dev      # run the desktop app with hot reload
npm run app:build    # release build + NSIS installer → src-tauri/target/release/bundle/nsis/
npm test             # frontend unit tests (Vitest)
npm run test:rust    # Rust unit tests
npm run typecheck
```

## Architecture

```
src/                     React + TypeScript UI (one bundle, two windows)
  dock/                  floating island, expand/collapse morph, drag, slots
  panels/                Music, Clipboard, Apps, Tasks, Calendar, Snippets, Wallpapers
  settings/              Settings window and first-run welcome
  lib/                   typed IPC contract (types.ts / ipc.ts), hooks, formatting
src-tauri/src/           Rust core
  dock.rs                dock controller actor: geometry, placement, focus, full-screen policy
  db.rs                  SQLite with versioned transactional migrations + pre-migration backup
  settings.rs            typed settings, validation, defaults
  services/              clipboard, apps, media (GSMTC), productivity, slots, snippets, wallpapers
  win/                   Win32/WinRT integration: monitors, window styles, clipboard,
                         OS event window (clipboard/display/power/session/full-screen), shell
  commands.rs            the typed command surface (validated, panic-guarded)
  state.rs               shared state, capture worker, reminder scheduler, app scan
  tray.rs                system tray
```

Design principles:

- **Event-driven.** No polling loops. The clipboard listener, display/power/session broadcasts, the full-screen app-bar notification, a foreground WinEvent hook and GSMTC events drive everything.
- **Fault isolation.** Each integration runs on its own thread with panic recovery. Commands convert panics into errors. Each React panel sits inside an error boundary.
- **Validated boundary.** All input is validated in Rust. App launches only use stored, validated references through the shell's default verb; the UI is never given a way to run arbitrary commands.
- **No click blocking.** The dock window is always exactly the visible island or panel.

## Documentation

- [docs/DECISIONS.md](docs/DECISIONS.md): answers to the PRD's open questions, plus other product decisions
- [docs/PRIVACY.md](docs/PRIVACY.md): what is stored, where, and how to delete it
- [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md): Windows versions, media players, known limitations
- [docs/QA-CHECKLIST.md](docs/QA-CHECKLIST.md): automated coverage, what was verified, and the manual test matrix

## License

Brim is proprietary software. Copyright © 2026 Kaustubh Chowdhury. All rights reserved.
This is not open-source software; see [LICENSE.txt](LICENSE.txt). The open-source components it uses are listed in [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
