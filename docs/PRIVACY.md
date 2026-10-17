# Privacy and data

Brim is local-first. It needs no account, uses no cloud service or AI provider, and sends no telemetry.

## What is stored, and where

| Data | Location |
|---|---|
| Settings, slot layout, clipboard history, tasks, reminders, calendar events, snippets, app list and launch history, wallpaper references | `%APPDATA%\dev.wispdock.desktop\wisp.db` (SQLite) |
| Imported wallpapers and their thumbnails | `%APPDATA%\dev.wispdock.desktop\wallpapers\`, `...\thumbnails\` |
| Pre-migration database backups | `%APPDATA%\dev.wispdock.desktop\wisp.db.v<N>.bak` |
| App icon cache | `%LOCALAPPDATA%\dev.wispdock.desktop\icons\` |
| Log file (no clipboard or personal content) | `%LOCALAPPDATA%\dev.wispdock.desktop\logs\` |

Settings › Privacy & data shows the storage location and sizes, and has a button to open the folder.

## Clipboard

- Capture is **off until you choose** on first run. You can turn it on, pause it (indefinitely or for 15 minutes), or turn it off from the dock, the tray menu or Settings.
- Only **text** is captured. Nothing is ever uploaded or synced.
- Never captured:
  - copies that the source app marks as private (most password managers do this);
  - copies from a program on your ignore list (pre-filled with common password managers);
  - when enabled, text that *looks like* a password, API key, token or private key.
- **Detection is not perfect.** The heuristics can miss secrets and can occasionally skip ordinary text. Pause capture before copying anything sensitive.
- Retention, item limits and size limits apply automatically (see `DECISIONS.md` §4). Pinned items are kept until you unpin or delete them.
- Clearing history also compacts the database, so deleted text does not linger in free pages.

## Deleting your data

- **Individual items** can be deleted from each panel.
- **Clipboard history** can be cleared from the dock or from Settings › Clipboard.
- **Settings › Privacy & data › Delete all local data** removes all of the following, then restarts first-run setup:
  - every database record and the pre-migration backups;
  - imported wallpapers and thumbnails;
  - the icon cache;
  - the launch-at-sign-in entry.
- **Uninstalling** (Windows Settings › Apps) removes the program and the launch-at-sign-in entry. The uninstaller has a "Delete the application data" checkbox; leave it unchecked to keep your data for a later reinstall. For a full manual removal, delete the two folders listed above.

## Network access

Brim makes no network requests of its own. The installer may download Microsoft's WebView2 runtime if it is missing.
