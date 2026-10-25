# Releasing Pencil Download Manager

## 1. One-time updater key setup

Tauri signs each update payload with a minisign-compatible key. The application embeds the public key and verifies every downloaded update against it. Keep the private key out of Git, releases, issues, logs, and this repository. Losing the private key means existing installations cannot trust future updates unless you ship a new installer that embeds a replacement public key.

1. Install the repository's npm dependencies with `npm ci`.
2. Generate a key pair locally:

   ```sh
   npm run tauri signer generate -- -w ~/.tauri/pdm-updater.key
   ```

   On Windows PowerShell, use `npm run tauri signer generate -- -w "$HOME/.tauri/pdm-updater.key"`. Choose a strong password when prompted and keep a secure offline backup of both files.
3. Open `~/.tauri/pdm-updater.key.pub` and copy its complete contents into `src-tauri/tauri.conf.json` at `plugins.updater.pubkey`, replacing `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY`. The public key is not secret. Commit this public-key change before creating the first updater-enabled release.
4. Add these repository Actions secrets under **Settings → Secrets and variables → Actions**:
   - `TAURI_SIGNING_PRIVATE_KEY`: the complete contents of `pdm-updater.key` (not the `.pub` file).
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the password used when generating the key.
5. Never upload the private key as an artifact or commit it. Keep a secure backup separate from the repository.

The source config intentionally starts with a public-key placeholder so a project owner can generate and control their own private key. Do not make an updater-enabled release until the placeholder has been replaced with the public key matching the GitHub secret.

### Rotating the updater key

A key rotation requires a transition release. First generate a new pair and preserve both pairs. Ship a release signed with the old private key whose embedded configuration trusts the new public key; then replace `TAURI_SIGNING_PRIVATE_KEY` and its password secret with the new pair for subsequent releases. Because this configuration embeds one public key, a direct one-step replacement would strand installations that only trust the old key. If a transition build cannot be signed by the old key, users must install a new installer manually.

## 2. Optional platform code-signing secrets

The release workflow supports unsigned builds when platform-signing secrets are absent. Updater signing is separate: without `TAURI_SIGNING_PRIVATE_KEY`, ordinary installers are built but updater artifacts and `latest.json` are disabled.

### Windows: signtool with an OV/EV PFX

For a certificate that can be exported as a PFX, add:

- `WINDOWS_CERTIFICATE`: base64-encoded PFX bytes (single line).
- `WINDOWS_CERTIFICATE_PASSWORD`: PFX export password.

Example encoding commands:

```sh
# Linux/macOS
base64 -w 0 certificate.pfx

# Windows PowerShell
[Convert]::ToBase64String([IO.File]::ReadAllBytes('certificate.pfx'))
```

The workflow's `src-tauri/windows/sign-windows.ps1` invokes Windows SDK `signtool.exe` with SHA-256 signing and a timestamp server. A hardware-backed EV certificate that cannot be exported to PFX may require the certificate vendor's signing tool/provider instead of this PFX path.

### Windows: Azure Trusted Signing

Instead of the PFX secrets, configure the Azure Artifact Signing / Trusted Signing account, identity and certificate profile, then add all of these Actions secrets:

- `AZURE_CLIENT_ID`
- `AZURE_CLIENT_SECRET`
- `AZURE_TENANT_ID`
- `AZURE_TRUSTED_SIGNING_ENDPOINT` (for example, the endpoint shown for your Azure signing account)
- `AZURE_TRUSTED_SIGNING_ACCOUNT`
- `AZURE_TRUSTED_SIGNING_CERTIFICATE_PROFILE`

The workflow installs `artifact-signing-cli` only when the full Azure configuration is present and no PFX is configured. Grant the identity the required signing permissions in Azure. The PFX option takes precedence if both are configured.

### macOS signing and notarization

Add all of the following repository secrets for Developer ID signing and notarization:

- `APPLE_CERTIFICATE`: base64-encoded `.p12` certificate.
- `APPLE_CERTIFICATE_PASSWORD`: password used to export the `.p12`.
- `APPLE_SIGNING_IDENTITY`: Developer ID Application identity.
- `APPLE_ID`: Apple ID email.
- `APPLE_PASSWORD`: Apple app-specific password.
- `APPLE_TEAM_ID`: Apple Developer Team ID.

Tauri uses these environment variables to sign and notarize the macOS bundles. If they are absent, the workflow builds unsigned bundles. The entitlements file at `src-tauri/Entitlements.plist` is configured in `tauri.conf.json`; it permits unsigned executable memory and disables library validation for the app's separately managed download-engine runtime. Review these entitlements if the engine distribution model changes.

## 3. Cut a release

1. Ensure CI is green on the commit to release.
2. Set the version everywhere with `node scripts/set-version.mjs 0.2.0` (SemVer; a leading `v` is accepted). It updates both `Cargo.toml` files, `Cargo.lock`, `package.json`, `package-lock.json`, `src-tauri/tauri.conf.json` and both extension manifests. Then add a `## 0.2.0` section to `CHANGELOG.md`; the release notes step looks it up by that heading.
3. Commit the version and changelog changes. `node scripts/check-versions.mjs --tag v0.2.0` confirms everything agrees.
4. Create and push a version tag, for example:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

5. The release workflow first checks that the tag matches the committed version and stops before creating a draft or building anything if it does not (installer file names come from the committed version, not the tag). If that happens, delete the tag, rerun `set-version.mjs`, commit, and tag again. It then builds Windows x64, Linux x64, macOS Intel and macOS Apple Silicon. Every matrix build uploads to a **draft** GitHub release. The final `publish` job runs only if the manifest check and every matrix build succeed; it checks for `latest.json` when updater signing is enabled and then publishes the draft.
6. Check the published release page. An updater-enabled release must contain `latest.json`, the platform update payloads and their `.sig` files. The release assets and `latest.json` must be publicly downloadable for the default updater endpoint to work.

Do not re-use a published tag for a different build. Create a new version and tag for each release. GitHub release downloads are used by the updater; private repositories are not anonymously readable by installed clients. For a fork test, change `plugins.updater.endpoints` to that fork's public release URL before building the test version. Do not embed a personal access token in the app to access a private release.

## 4. Test an update from version N to N+1

Use a disposable test machine or VM and a public test fork with its own updater key. Do not test by replacing the public key while keeping the old private key.

1. In the fork, generate its own updater key pair and set the matching public key in `src-tauri/tauri.conf.json`. Add the private key and password as the fork's Actions secrets. Change the updater endpoint to `https://github.com/OWNER/Pencil-Download-Manager/releases/latest/download/latest.json`, using the fork owner/repository.
2. Build and publish version **N** with the fork workflow. Download and install that release's installer on the test machine. Confirm About → Check for updates reports that no update is available.
3. Increment the app version to **N+1** with `node scripts/set-version.mjs`; add a `CHANGELOG.md` entry; commit and push a new tag such as `v0.2.1`.
4. Wait for all release matrix jobs and the final publish job to succeed. Confirm `latest.json` includes the current version and platform entries and that its payload URLs and signatures are present in the release assets.
5. Launch the installed N build and select About → **Check for updates**. Confirm the N+1 changelog appears. Choose **Restart to update**; if downloads are active or queued, the app must ask for confirmation before installing. After restart, confirm About reports N+1.
6. Also test the unsigned path by running a release workflow without `TAURI_SIGNING_PRIVATE_KEY`: installers should still build and the release should publish, but updater artifacts and `latest.json` are intentionally omitted. This verifies unsigned installation compatibility, not automatic updates.

The default GitHub `releases/latest/download/latest.json` endpoint requires a public release. A private repository's release assets cannot be fetched anonymously by a desktop client. For private testing, provide an HTTPS update endpoint that authenticates clients safely; do not put a repository PAT in `tauri.conf.json` or ship it in the app.

## Release checklist (installers only)

Releases are always created as drafts by the release workflow; nothing publishes them automatically. Before publishing a draft by hand, confirm on the actual draft release page:

- [ ] Assets are exactly: Windows NSIS `.exe`; macOS arm64 and x64 `.dmg`; Linux `.deb` and `.rpm`; plus `aria2-<version>.tar.xz`, `COPYING`, `ENGINE-SOURCE.txt` and `SHA256SUMS`. The `verify-assets` job passed.
- [ ] aria2 source tarball and COPYING attached.
- [ ] Hashes verified: `sha256sum -c SHA256SUMS` passes on the downloaded assets, and the `engine-manifest.json` pins match upstream (`verify-manifest` job).
- [ ] Signatures checked (Windows Authenticode, macOS codesign/notarization) where signing secrets were configured, by inspecting the downloaded installers.
- [ ] Windows and macOS installers contain aria2 and `licenses/aria2/COPYING`; the `.deb` and `.rpm` declare `Depends: aria2`.
- [ ] No portable or unpacked artifacts attached (no `.zip`, `.tar.gz`, `.AppImage`, loose `.exe`, `.app` folders, `.nsis.zip`).
- [ ] `docs/test-checklist.md` has been run on real machines, with results recorded.

