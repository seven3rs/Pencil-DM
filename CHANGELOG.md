# Changelog

## Unreleased

- App interface restyled to match the website: same colors, Inter and JetBrains Mono (bundled, no network fonts), logo and gradient progress bars.
- New **Browser** page with a guided extension install (prepares a stable extension folder, opens Chrome, Edge, Brave, Vivaldi or Firefox at the right page) and live connection status.
- Browser pairing now survives app restarts: the token and paired extension are kept, and the extension re-discovers the port. **Disconnect** resets it.
- Fixed the extension popup always reporting "not connected" and pairing failures going unnoticed in Chrome.
- A busy discovery port no longer stops the whole app from starting.
- Fixed the tray: right-clicking the icon no longer re-opens the hidden window or closes the menu; only a left click restores the window.
- A failed engine poll no longer leaves a stuck error banner.
- Release workflow: new `scripts/set-version.mjs` bumps the version in every file at once, and the workflow now refuses a tag that does not match the committed version (previously a `v0.1.1` tag produced installers named 0.1.0).
- Release workflow: fixed the bundle folder used by the installers-only guard and skipped empty Apple signing secrets.

## 0.1.0

- Initial development release of Pencil Download Manager.
- Rust/Tauri desktop shell and a verified, locally controlled download engine.
- Windows pinned engine manifest, basic download controls, and CI/release workflows.
- Svelte 5 + TypeScript interface: sidebar filters, multi-link Add dialog, virtualized download table, details panel, keyboard shortcuts, context menu, and light/dark themes.
- aria2 1.37.0 is bundled in the Windows and macOS installers (Windows: the official release binary; macOS: built from the pinned source for each architecture). Linux packages depend on the system aria2.
- The bundled engine is hash-checked at launch; if it is missing or modified the app falls back to a verified download and reports it.
- Releases are installers only: Windows `.exe` (NSIS), macOS `.dmg`, Linux `.deb` and `.rpm`, plus the aria2 source tarball, its COPYING file, `ENGINE-SOURCE.txt` and `SHA256SUMS`.
- About screen lists aria2's version, license and source link. New **Delete all app data** action. The Windows uninstaller removes the bundled and downloaded engine and can optionally delete history and settings.
