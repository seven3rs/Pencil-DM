# Download engine installation

**Windows and macOS** installers include aria2 (version pinned in `engine-manifest.json`), so nothing needs to be installed. The app verifies the bundled binary against a hash built into the app on every start. If it is missing or modified, the app does not run it and falls back to a verified download into the per-user data folder, showing a message in the engine panel.

**Linux** packages do not include aria2; they depend on the system `aria2` package. The rest of this page describes how the app finds and checks a system aria2 (used on Linux, and when you choose your own path on macOS).

When a system aria2 is used, the app does not download a binary. The app refreshes the minimum accepted version from the latest stable upstream release metadata on GitHub at startup. The `min_system_version` in `engine-manifest.json` (`1.36.0`) is an offline fallback used if the metadata request fails or the machine has no internet connection. The metadata request has a short timeout and does not download binaries or hashes.

At startup, the app checks the executable found on `PATH` and common locations including `~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, and `/usr/bin`. It invokes the candidate with `--version` and rejects candidates whose version cannot be parsed or is older than the latest stable upstream release (or the embedded fallback when offline). This means the version requirement advances automatically when upstream publishes a newer stable release; package-manager availability may lag behind upstream.

If no compatible executable is found, the UI shows installation guidance:

- macOS with Homebrew: `brew install aria2`
- Debian/Ubuntu: `sudo apt install aria2`
- Fedora/RHEL-family: `sudo dnf install aria2`

The app offers a **Use my own engine path** setting. The selected absolute path is stored in the app data directory as `engine-path.txt`. The path is checked by running its version command before it is saved. The file contains only the path and is local to the machine.

Only version metadata is refreshed at runtime. Trusted download URLs and hashes remain embedded at compile time and are never replaced by the version metadata endpoint.
