# Distributing Brim

## Build a release

```bash
npm run release
```

This builds the installer and writes `release/v<version>/`:

| File | Purpose |
|---|---|
| `Brim_<version>_x64-setup.exe` | The installer to share (≈5 MB) |
| `SHA256SUMS.txt` | Checksum, so people can verify the download |
| `How to install.txt` | Plain-language instructions for recipients (from `docs/INSTALL.txt`) |

If the default build folder is locked (for example, Brim is running from `src-tauri/target`), build elsewhere:

```bash
CARGO_TARGET_DIR=src-tauri/target-dist npm run release
```

Release builds of the Windows API bindings need a few GB of free memory. Close games and other heavy apps first.

## What the installer does

- **Install mode:** asks whether to install just for the current user (no admin rights; installs under `%LOCALAPPDATA%`) or for everyone on the PC (admin rights; installs to `Program Files`).
- **Shortcuts:** adds a Start menu entry under "Brim".
- **Finish page:** can launch the app straight away.
- **WebView2:** bundles Microsoft's WebView2 bootstrapper (a signed Microsoft installer). It sets up the runtime silently on PCs that don't have it, which mostly means older Windows 10; it needs internet for that step.
- **Uninstaller:**
  - removes the "launch at sign-in" entry;
  - has a "Delete the application data" checkbox, unchecked by default, so user data is kept.

## Code signing (recommended before sharing widely)

The installer is **not signed**. Recipients will see a SmartScreen warning ("Windows protected your PC") and must click *More info → Run anyway*. To remove the warning you need a code-signing certificate in your name or your organisation's. Options:

| Option | Notes |
|---|---|
| **Azure Trusted Signing** | Low monthly cost, no hardware token. Requires identity verification. Use Tauri's `bundle.windows.signCommand` with Microsoft's `trusted-signing-cli`. |
| **OV/EV certificate** from a CA (DigiCert, Sectigo, SSL.com, …) | Usually issued on a hardware token or in cloud HSM signing. EV builds SmartScreen reputation fastest. |

Once you have a certificate installed in the Windows certificate store, add this to `src-tauri/tauri.conf.json` under `bundle.windows` and rebuild:

```json
"certificateThumbprint": "<your certificate's SHA-1 thumbprint>",
"digestAlgorithm": "sha256",
"timestampUrl": "http://timestamp.digicert.com"
```

Tauri then signs both the app and the installer. For Azure Trusted Signing or cloud HSMs, use `signCommand` instead (see the Tauri docs on Windows code signing).

Even signed installers from a new publisher can show SmartScreen for a while until the certificate builds reputation.

## Before each release

1. **Bump the version** in both `src-tauri/tauri.conf.json` and `package.json`. Installing a newer version over an older one upgrades in place and keeps user data.
2. **Run the checks:** `npm run typecheck`, `npm test`, `npm run test:rust`.
3. **Smoke-test the installer** on a second PC or a clean Windows VM (see `QA-CHECKLIST.md`).
4. **Publisher name:** recipients see the publisher in Apps & features. It is currently "Brim". Change `bundle.publisher` if you want your own name or company shown.
