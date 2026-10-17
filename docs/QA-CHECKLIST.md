# QA checklist

## Automated coverage

- **Rust unit tests** (`npm run test:rust`, 50 tests):
  - schema migrations: v1→v2 data preservation, a backup written before migrating, a newer-version database refused untouched;
  - settings: validation and patching;
  - clipboard: dedup, retention by age/count/size, pins, search, secret heuristics;
  - app launcher: discovery merge, missing apps and paths, recents and exclusion;
  - slots: assignment, validation, move, reset;
  - tasks, reminders (exactly-once delivery across restarts) and calendar;
  - snippets;
  - wallpapers: import, dedup, missing files, removal;
  - dock placement maths: fallback placement, clamping, expand direction.
- **Frontend unit tests** (`npm test`, 9 tests): date/time formatting and parsing, month grid, media position interpolation.

## Verified on the development machine

Windows 11 26200, one 1920×1080 monitor at 100%, auto-hiding taskbar.

- [x] Dock starts once, hidden from the taskbar and Alt+Tab (`WS_EX_TOOLWINDOW`), topmost, top-centre by default.
- [x] Clicks 2 px outside the island and outside the panel reach the window underneath (`WindowFromPoint` test).
- [x] The global shortcut expands the dock. Escape collapses it, and focus returns to the previous app.
- [x] A blur caused by focus moving into the webview does not collapse the panel.
- [x] Maximised windows with an auto-hiding taskbar are not treated as full-screen.
- [x] A taken default shortcut falls back to a free one at startup.
- [x] The Start menu scan finds desktop and Store apps (173 entries).

## Manual test matrix still to run

Use a clean user profile where possible.

### Environments

- [ ] Windows 10 22H2 (WebView2 bootstrapper path on a machine without the runtime).
- [ ] Two monitors with **different scaling** (for example 100% and 150%). Drag the dock between them, collapse and expand on each, then restart.
- [ ] Unplug the dock's monitor while the app is running. The dock should move to the primary monitor, then return when the monitor is plugged back in.
- [ ] Change resolution and scaling while the app is running.
- [ ] Sleep/wake, lock/unlock, sign-out/sign-in, restart with "launch at sign-in" on.
- [ ] Full-screen video in a browser (F11 and the video full-screen button), a full-screen game, and a PowerPoint slideshow. The dock should hide on that monitor only.
- [ ] No network connection: everything except the WebView2 bootstrapper works.

### Media

- [ ] Spotify, Edge/YouTube, Chrome/YouTube, Firefox and Windows Media Player:
  - [ ] title, artist and artwork shown;
  - [ ] play/pause, next and previous work;
  - [ ] progress and seeking work.
- [ ] Two players at once: the session picker appears; "Automatic" follows Windows' current session.
- [ ] Close the player while it is shown: the "player closed" state appears, then the empty state.

### Clipboard

- [ ] Copy from KeePassXC or Bitwarden: not captured, and the notice explains why.
- [ ] Copy a 3 MB text: the "too large" notice appears and nothing is stored.
- [ ] Copy the same text twice: one entry, moved to the top.
- [ ] Pause for 15 minutes: capture resumes automatically and the tray text updates.

### Apps

- [ ] Uninstall a pinned app: it shows as unavailable, launching explains the problem and offers removal, and the dock does not crash.

### Reminders

- [ ] Fires once.
- [ ] After dismissal and a restart, it never fires again.
- [ ] Missed while the app was closed (under 24 h): delivered once on start.

### Installer

- [ ] Install → uninstall with the data checkbox unchecked: data kept, startup entry removed.
- [ ] Reinstall: data restored.
- [ ] Uninstall with the data checkbox checked: `%APPDATA%\dev.wispdock.desktop` removed.

### Accessibility

- [ ] Keyboard only: open with the shortcut, Tab and arrow keys through the panels, Enter to act, Escape to close.
- [ ] Screen reader (Narrator) announces slot and button names.
