# Changelog

## 0.1.0

- Initial development release of Pencil Download Manager.
- Rust/Tauri desktop shell and a verified, locally controlled download engine.
- Windows pinned engine manifest, basic download controls, and CI/release workflows.
- Svelte 5 + TypeScript interface: sidebar filters, multi-link Add dialog, virtualized download table, details panel, keyboard shortcuts, context menu, and light/dark themes.
- aria2 1.37.0 is bundled in the Windows and macOS installers (Windows: the official release binary; macOS: built from the pinned source for each architecture). Linux packages depend on the system aria2.
- The bundled engine is hash-checked at launch; if it is missing or modified the app falls back to a verified download and reports it.
- Releases are installers only: Windows `.exe` (NSIS), macOS `.dmg`, Linux `.deb` and `.rpm`, plus the aria2 source tarball, its COPYING file, `ENGINE-SOURCE.txt` and `SHA256SUMS`.
- About screen lists aria2's version, license and source link. New **Delete all app data** action. The Windows uninstaller removes the bundled and downloaded engine and can optionally delete history and settings.
