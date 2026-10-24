# Pencil Download Manager

Pencil Download Manager is an early-stage cross-platform desktop download manager built with Rust and Tauri v2. The current interface is a lightweight HTML/CSS/JavaScript prototype; the full feature set described in the agent playbook is not yet implemented.

## Project layout

- `crates/engine`: verified engine installation, process launcher, and local JSON-RPC client.
- `src-tauri`: desktop shell and commands exposed to the UI.
- `ui`: current prototype frontend.
- `engine-manifest.json`: pinned engine version, URLs, and hashes embedded at compile time.
- `scripts/verify-manifest.sh`: verifies pinned upstream artifacts.
- `.github/workflows`: CI and draft release builds.

## Engine setup

The Windows installer installs the download engine from the embedded manifest, checks the downloaded archive SHA-256, safely extracts it, checks the binary SHA-256, and installs it in the app data directory. On launch, the managed binary is checked again and repaired if missing or modified. Linux packages currently declare the system engine as a package dependency.

The app controls the engine as a local child process over JSON-RPC bound to loopback with a per-launch random secret. Network-provided names and errors must always be rendered as text, never HTML.

## Development

Requirements: stable Rust toolchain, Node.js 22/npm, and Tauri system dependencies. On Ubuntu/Debian install `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, and `patchelf`.

```sh
npm ci
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p pdm-engine
bash scripts/verify-manifest.sh
npm run tauri dev
npm run tauri build
```

## Security and reporting issues

See [SECURITY.md](SECURITY.md), [the threat model](docs/threat-model.md), and [security findings](docs/security-findings.md). Do not include private download URLs, credentials, or personal data in bug reports.

## Release status

This is a development build, not a production-ready 0.1.0 release. The browser extension, persistence and categories, scheduler, tray polish, media extraction, signed updater, full Svelte UI, and end-to-end desktop tests remain incomplete. The release workflow builds draft artifacts; verify each artifact and platform manually before publishing. See [docs/releasing.md](docs/releasing.md) and [docs/test-checklist.md](docs/test-checklist.md).
