# Clean-machine test checklist

Manual, hands-on testing that no CI job covers. Record OS/version, app version, date and result for each run. Do not mark an item passed unless it was observed on a real machine.

Status: **not run.** Nothing below has been executed on a real Windows, macOS or Linux desktop yet.

- [ ] Install with internet access; first launch reaches **Ready**.
- [ ] Install fully offline. Windows/macOS: the app opens and the bundled engine works. Linux: a clear missing-engine message appears and clears after installing `aria2`.
- [ ] Bundled engine file deleted: falls back to the managed download (or shows a clear error), and repairs when online.
- [ ] Bundled engine file modified: the hash mismatch is detected, the file is never run, and a clear message is shown.
- [ ] Antivirus quarantines the engine: clear message and recovery steps.
- [ ] Windows uninstall: the install folder and any downloaded engine are removed; the history/settings prompt defaults to keep; `/S /DELETEDATA` deletes them silently; downloaded files are untouched.
- [ ] macOS and Linux: **About → Delete all app data** removes history, settings and the managed engine, quits the app, and leaves downloaded files alone.
- [ ] `.deb` / `.rpm` removal deletes only `~/.local/share/strs.pencil.downloadmanager/engine`; upgrading does not remove it.
- [ ] Upgrade over an older version keeps data.
- [ ] Non-ASCII OS username works.
- [ ] Install and download paths containing spaces work.
- [ ] 125% display scaling is usable.
- [ ] High-contrast / system accessibility settings remain usable.
- [ ] Keyboard-only operation works and focus is visible.
- [ ] About screen shows the aria2 version, license, source link and "not affiliated" line; the source link opens.
- [ ] Invalid and non-network URL schemes are rejected.
- [ ] The release page holds only installers plus `aria2-<version>.tar.xz`, `COPYING`, `ENGINE-SOURCE.txt` and `SHA256SUMS`: no portable, zipped or unpacked artifacts.
- [ ] Open the Windows installer and the `.dmg` and confirm aria2 and `licenses/aria2/COPYING` are inside; inspect the `.deb`/`.rpm` metadata for the `aria2` dependency.
