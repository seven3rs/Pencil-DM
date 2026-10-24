# Third-party notices

Pencil Download Manager is licensed under GPL-3.0-or-later (see [LICENSE](LICENSE) and [LICENSING.md](LICENSING.md)). It uses the third-party software below, each under its own license.

## Download engine: aria2

Pencil Download Manager uses a separate download-engine program that is downloaded and verified at install time. It is not part of this repository and is not linked into the app.

- **aria2**: GNU GPL v2 or later, with a special exception allowing linking with OpenSSL (see the `COPYING` file in the aria2 release)
  - Website: https://aria2.github.io/
  - Source code: https://github.com/aria2/aria2 (the release tag for each version is named `release-<version>`)
  - The exact release and its checksums are pinned in `engine-manifest.json`.

The app starts aria2 as its own process and controls it over local JSON-RPC. For that reason this project's code is not a derivative work of aria2 and keeps its own license.

**If you redistribute the aria2 binary yourself** (for example in an offline installer or a package):

- include aria2's license text and this notice, or an equivalent entry in the app's About screen;
- provide, or offer, the exact corresponding aria2 source for that version, for example the upstream source tarball for the version pinned in `engine-manifest.json`.

aria2 is a project of its respective authors. Pencil Download Manager is not affiliated with or endorsed by them.

## Application framework and tooling

Licenses as published by each project. The generated lists below are authoritative.

| Component | License |
|---|---|
| Tauri | Apache-2.0 or MIT |
| Svelte | MIT |
| Vite | MIT |
| TypeScript | Apache-2.0 |

## System components

- **Windows:** the app uses Microsoft Edge WebView2, which is provided under Microsoft's own terms.
- **Linux:** the app uses WebKitGTK from the system, under its own licenses (LGPL and others).

## Full dependency lists

Rust crates and npm packages are listed with their licenses in the generated files below. Regenerate them before every release.

- `THIRD_PARTY_RUST.md`: generate with `cargo about generate`
- `THIRD_PARTY_NPM.md`: generate with `npx license-checker --production --markdown`

Every dependency must be compatible with GPL-3.0-or-later. See the compatibility list in [LICENSING.md](LICENSING.md).
