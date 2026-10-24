# Releasing

## Current workflow

1. Run formatting, clippy, tests, manifest verification, and frontend checks on a clean checkout.
2. Update all version declarations together (`Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and `package.json`).
3. Review `THIRD_PARTY_NOTICES.md` and `CHANGELOG.md`.
4. Push a version tag such as `v0.1.0`; the current workflow produces a draft release for manual review.
5. Verify every expected artifact before publishing. Do not publish missing or untested platform artifacts.

## Signing and updater status

The signed updater and platform signing/notarization are not yet wired up. Do not add private keys or certificates to the repository. Once updater support is implemented, generate the key pair using the official Tauri documentation, store the private key and password in repository secrets, and keep only the public key in app configuration. Test an update from version N to N+1 in a disposable environment before publishing.
