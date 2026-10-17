# Compatibility and known limitations

## Windows

- **Supported:** Windows 10 1809 (build 17763) or later, and Windows 11.
- **Requires the Microsoft Edge WebView2 runtime.** It is preinstalled on Windows 11; the installer downloads it on Windows 10 if needed.
- **Display scaling:** per-monitor DPI aware (v2). Sizes follow each monitor's scale factor, and the dock re-places itself after resolution, scaling, work-area and monitor changes.

## Media players

Media controls use the Windows *Global System Media Transport Controls* (GSMTC) sessions, the same ones that drive the volume flyout's media panel. Any app that publishes a session works, but how much it exposes varies:

| Player | Title / artist | Artwork | Play/pause, next/previous | Progress & seek |
|---|---|---|---|---|
| Spotify (desktop app) | Yes | Yes | Yes | Position updates are infrequent; the dock interpolates between them. Seeking is supported. |
| Microsoft Edge / Google Chrome (YouTube, etc.) | Yes | Usually | Depends on the site | Depends on the site's Media Session support |
| Firefox | Yes | Usually | Depends on the site | Depends on the site |
| Windows Media Player (new) | Yes | Yes | Yes | Yes |

- **Not universal.** A player that doesn't publish a GSMTC session (some older or portable players) cannot be controlled. Controls the player doesn't support are shown disabled, with an explanation.
- **Multiple sessions.** The dock uses, in order:
  1. the session you picked;
  2. Windows' current session;
  3. the first playing session;
  4. the first session.
- **Unresponsive players.** Every call to a player has a 2.5-second timeout, so a frozen player can't freeze the dock.
- **Unavailable service.** Windows "N" editions without the Media Feature Pack may not provide media sessions. The panel says so.

## Known limitations

- **Reminders need the app running.** Missed reminders are delivered once at next start if less than a day late.
- **Notifications in development builds** use PowerShell's app identity. Installed builds use their own.
- **No desktop blur.** Windows doesn't blur content behind a transparent WebView2 window, so the panel uses a near-opaque surface instead. Opacity is adjustable in Settings › Appearance.
- **Focus hand-back** after closing the dock uses `SetForegroundWindow`. Windows' focus-stealing rules can occasionally refuse it, for example if you clicked elsewhere in the meantime.
- **Shortcut conflicts.** Global shortcuts already owned by other programs can't be registered. The default falls back automatically (see `DECISIONS.md`), and a shortcut you chose shows a warning in Settings if it's unavailable.
- **Wallpapers:** JPG, PNG and BMP up to 50 MB. "Set as wallpaper" applies to all monitors.
- **Apps list:** built from the Start menu's app list (desktop and Store apps). Programs without a Start menu entry can be added manually with **+**.
