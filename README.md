# Pencil Download Manager

Pencil Download Manager is an open-source desktop download manager for Windows, macOS and Linux, built with Rust and Tauri v2 and a Svelte 5 + TypeScript interface. Downloads are performed by [aria2](https://aria2.github.io/), a separate program the app starts and controls locally.

> Status: 0.1.0 is the first release. Screenshots will be added under `docs/assets/` once they are captured from a running build.

## Install

Releases are **installers only**. Download the one for your system from the [Releases page](https://github.com/seven3rs/Pencil-Download-Manager/releases) and run it.

| System | Installer |
|---|---|
| Windows (x64) | `.exe` installer (per-user, no administrator rights needed) |
| macOS (Apple silicon and Intel) | `.dmg` (drag the app to Applications) |
| Linux | `.deb` (Debian/Ubuntu) or `.rpm` (Fedora/RHEL family) |

Each release also lists `SHA256SUMS`, the exact aria2 source tarball used, and aria2's `COPYING` file. Compare the checksum of what you downloaded against `SHA256SUMS` before installing.

## The download engine (aria2)

- **Windows and macOS:** aria2 is included in the installer. The app checks it against a hash built into the app each time it starts. If it is missing or has been modified, the app does not run it; it falls back to a verified download into your app data folder and says so in the engine panel.
- **Linux:** aria2 is not bundled. The `.deb` and `.rpm` declare it as a package dependency, and the app uses the system copy. See [engine installation](docs/engine-installation.md).
- You can point the app at your own aria2 executable on macOS and Linux (Use my own engine path).

The app talks to aria2 only over a loopback JSON-RPC connection protected by a random secret created for each launch. Names and errors that come from the network are always shown as text, never as HTML. The About screen lists the bundled aria2 version, its license, and a link to its source code. This project is not affiliated with the aria2 project.

## Uninstalling and deleting your data

- **Windows:** the uninstaller removes the app and the bundled engine, and asks whether to delete history and settings (the default is to keep them). For silent uninstalls, pass `/DELETEDATA` to delete them.
- **macOS and Linux:** removing the app cannot ask. Use **About → Delete all app data** inside the app first, or delete the folders below by hand.
- Files you downloaded are never deleted by the uninstaller or by Delete all app data.

App data folders (history, settings, schedules, a downloaded engine):

| System | Folder |
|---|---|
| Windows | `%LOCALAPPDATA%\strs.pencil.downloadmanager` |
| macOS | `~/Library/Application Support/strs.pencil.downloadmanager` |
| Linux | `~/.local/share/strs.pencil.downloadmanager` |

Removing the `.deb` or `.rpm` deletes only the managed engine subfolder (`engine`) in each user's data folder, never anything else.

## Project layout

- `crates/engine`: engine resolution and verified installation, process launcher, and the local JSON-RPC client.
- `src-tauri`: the desktop shell and the commands exposed to the UI.
- `ui`: Svelte 5 + TypeScript + Vite frontend (`src/lib` holds testable logic, `src/components` the views).
- `extension`: browser extension.
- `engine-manifest.json`: pinned aria2 version, URLs and hashes, embedded at compile time.
- `scripts/`: `fetch-engine.mjs` (fetch or build the aria2 sidecar), `verify-manifest.sh`, `check-versions.mjs`, `check-release-assets.mjs`.
- `.github/workflows`: CI and draft release builds.

## Build from source

Requirements: stable Rust, Node.js 22 with npm, and the Tauri system dependencies. On Debian/Ubuntu install `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev` and `patchelf`.

```sh
npm ci
npm run check
npm test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/verify-manifest.sh
node scripts/fetch-engine.mjs      # Windows/macOS: fetch or build the aria2 sidecar
npm run tauri dev
```

Notes:

- `npm ci` can fail on Windows or macOS because `package-lock.json` pins only Linux's optional rollup binary (npm/cli#4828). Use `npm install` followed by `node scripts/ensure-rollup-native.mjs` there.
- aria2 is never committed to the repository. `fetch-engine.mjs` downloads pinned, hash-verified inputs into `src-tauri/binaries/` and `src-tauri/resources/licenses/`, which are git-ignored. On macOS it builds aria2 from the pinned source tarball.

## Contributing

Contributions are welcome. Sign off every commit (`git commit -s`) to certify the Developer Certificate of Origin. Run the checks above before opening a pull request; CI runs them on Linux, Windows and macOS.

## License and security

Pencil Download Manager is licensed under GPL-3.0-or-later; see [LICENSE](LICENSE) and [LICENSING.md](LICENSING.md). Trademark rules are in [TRADEMARK.md](TRADEMARK.md), and bundled third-party software is listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md); also see [the threat model](docs/threat-model.md). Do not include private download URLs, credentials or personal data in bug reports.

## Releasing

See [docs/releasing.md](docs/releasing.md) and the manual [clean-machine test checklist](docs/test-checklist.md).
