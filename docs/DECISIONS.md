# Product decisions

Answers to the open questions in PRD §18, as implemented. Each one names where it lives in the code.

| # | Question | Decision |
|---|---|---|
| 1 | Minimum Windows builds | **Windows 10 1809 (build 17763)** and **Windows 11 21H2 (build 22000)**. 1809 is the first build with both WebView2 support and the Global System Media Transport Controls API used for media. |
| 2 | Always-on-top / full-screen policy | **Always on top by default, user-configurable.** The dock **hides while a full-screen app is in front on its monitor** (configurable). A window counts as full-screen when it covers its whole monitor *and* is either borderless and not maximised, or the shell itself reports a full-screen/presentation/D3D app. The second condition stops maximised windows from counting when the taskbar auto-hides. The shortcut still opens the dock over a full-screen app. Detection is event-driven: `ABN_FULLSCREENAPP` plus a foreground WinEvent hook. See `win/window.rs` and `dock.rs`. |
| 3 | Default clipboard capture on first run | **Off until the user chooses.** The welcome screen has no default selection; the user must pick "Keep clipboard history" or "Not now" before continuing. |
| 4 | Retention presets and size limits | Retention 1 / **7 (default)** / 30 / 90 days or no time limit. History size 50 / **200 (default)** / 500 / 1000 items. Largest item 64 KB / **256 KB (default)** / 1 MB / 2 MB. Unpinned history is also capped at **25 MB** in total. Up to 100 pinned items, never removed by cleanup. |
| 5 | Text only or images | **Text only** for MVP. |
| 6 | Compact dock shows all four slots? | **Yes**, all four slots are always visible in the compact island. |
| 7 | Fallback when the saved monitor is gone | The position is saved **relative to the monitor's work area** together with a monitor id. If that monitor is missing, the dock goes to the **primary monitor at the same relative position**, clamped fully on screen. The saved monitor is not overwritten by the fallback, so the dock returns when the monitor reconnects. |
| 8 | Official media compatibility targets | **Spotify (desktop), Microsoft Edge, Google Chrome, Firefox, Windows Media Player.** See `COMPATIBILITY.md`. |
| 9 | Light theme in MVP? | **Yes.** Dark (default), Light and System. |
| 10 | Reminder delivery mechanism | **The app must be running.** Reminders missed while it was closed are shown once on next start if they are less than 24 hours late; older ones are marked *missed* silently and listed in Tasks. Delivery is at-most-once: a database claim happens before the notification is shown. |
| 11 | Wallpaper import: copy or reference? | **Copy** into the app data folder, named by content hash. Moving or deleting the original never breaks the gallery, and duplicates are detected. |
| 12 | Uninstall and the database | **Retained by default, with a choice.** The NSIS uninstaller offers a "Delete the application data" checkbox, unchecked by default. The uninstaller always removes the launch-at-sign-in registry entry. |
| 13 | Opt-in diagnostics for beta | **None collected.** A local log file with no clipboard or personal content is written to `%LOCALAPPDATA%\dev.wispdock.desktop\logs`, and users can attach it to support requests themselves. |
| 14 | Snippets in MVP? | **Included** in the first usable release (roadmap phase 2), alongside the five-feature MVP. |

## Other notable decisions

- **Default shortcut `Alt+Shift+Space`.** `Ctrl+Alt+Space` is commonly taken by other apps (it was taken on the development machine). If the default is unavailable and the user has never chosen one, the first free fallback is used and saved: `Ctrl+Alt+Backquote`, `Ctrl+Shift+Space`, then `Alt+Backquote`. A shortcut the user chose is never replaced; Settings shows a warning instead.
- **Idle forms.** When idle the dock collapses out of the way, in one of two forms:
  - **Vanish** (default): the island shrinks to a point and disappears. An invisible 120×16 hover strip stays where it was.
  - **Line**: the island folds into a thin 100×4 line on its top edge, inside a 196×12 hover strip.

  Hovering grows the island back. The hover strips are kept small because transparent areas of the window would otherwise block clicks. While the island is shown in either form, the backend checks the cursor position every 150 ms and collapses the island again once the cursor has been away for 0.7 s. No checks run while it is collapsed. Existing installs are switched to Vanish once through a `defaultsVersion` settings upgrade (an earlier "dot" value is read as Vanish); choosing another mode afterwards is respected. Other modes: Visible, Faded, Hidden.
- **Live activities.** When a song starts or changes, the dock grows from its centre into a 360×64 "Now Playing" pill (artwork, title, artist, play/pause, next) for 5 s. Volume changes made outside the dock (keyboard keys, other apps) show a volume pill for 1.8 s. Either stays up while the cursor is on it, never takes keyboard focus, and is never shown while the panel is open or a full-screen app is in front. Volume uses Core Audio change notifications and follows the default output device. Changes the dock makes itself are tagged so they don't trigger the pill. Both pills can be turned off in Settings. **Limitation:** Windows 11 does not let other apps turn off its own volume pop-up, so both can appear on Windows 11.
- **Aqua theme (default).** An aqua-to-white gradient with a glossy top sheen; it pairs with a teal accent unless a custom accent is chosen. Existing installs were switched to it once (`defaultsVersion` 2).
- **Wide layout + Midnight theme (default).** The panel is 720×420 with a row of text tabs: Controls (media + volume), Today (agenda + due tasks), Stats, Clipboard, Apps, Snippets, Wallpaper and Power. Midnight is a near-black gradient with an amber accent. Existing installs were switched once (`defaultsVersion` 3). The Compact layout (400×540 with slot icons) and other themes remain in Settings › Appearance. In the wide layout, slots for Music, Tasks and Calendar open the matching tab (Controls / Today).
- **Stats** are read only while the Stats tab is open (once a second): CPU from `GetSystemTimes` deltas, physical memory, system-drive space, battery and uptime. Nothing samples in the background.
- **Power** offers Lock, Sleep, Restart, Shut down and Sign out. Restart, shut down and sign out need a second click within 4 s and run Windows' own `shutdown.exe` with fixed arguments, so other apps can save their work.
- **Desktop music widget (off by default).** A separate frameless tool window showing the current track as a navy glass card, in a tall "Card" or wide "Compact" style. It is draggable, remembers its position (clamped onto a visible monitor), can sit behind or above windows, and hides when nothing is playing (configurable). Windows' media sessions don't expose the playback queue, so there is no "Up next" view.
- **Volume keys (on by default).** Windows 11 has no setting to hide its volume flyout; it appears when Windows itself handles the volume keys. A low-level keyboard hook on its own thread swallows only VK_VOLUME_UP/DOWN/MUTE and applies the change through Core Audio (2% steps, like Windows), so only the dock's pill appears. The callback only forwards to a channel, so other keys are never delayed. Turning the setting off removes the hook.
- **Wallpaper Engine.** Found via Steam (`HKCU\Software\Valve\Steam\SteamPath` + `libraryfolders.vdf`). Wallpapers come from the Workshop folder (app 431960) plus `projects/myprojects` and `projects/defaultprojects`, read from each `project.json`. Applying and play/pause/mute use Wallpaper Engine's documented `-control` command line. Paths passed to it come only from the app's own scan.
- **Web search** opens YouTube or Google results in the default browser. The URL is built in Rust from a fixed engine and an encoded query. Results aren't shown in-app: YouTube's API needs a key and blocks embedding, and scraping is unreliable and against its terms.
- **The window is exactly the visible island.** Collapsed, the dock window is the size of the island, with no transparent margins. Expanded, it is the size of the panel. Transparent regions therefore never block clicks (FR-DCK-008). This was verified with `WindowFromPoint` 2 px outside each edge.
- **Hidden from Alt+Tab and the taskbar.** The dock uses `WS_EX_TOOLWINDOW`.
- **Recent apps** are recorded only for launches made from the dock (no foreground-window tracking), which keeps app history minimal.
- **Sensitive-content protection, in three layers:**
  1. Content that apps mark as private (`ExcludeClipboardContentFromMonitorProcessing`, `Clipboard Viewer Ignore`, `CanIncludeInClipboardHistory = 0`) is always skipped.
  2. A per-program ignore list, pre-filled with common password managers.
  3. Optional pattern and entropy heuristics for keys and passwords, which are explicitly best-effort.
