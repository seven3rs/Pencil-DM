# Clean-machine test checklist

Record OS/version, app version, date, and result for each run. Do not mark an item passed unless it was manually observed.

- [ ] Online install and first launch reaches **Ready**.
- [ ] Offline install opens and gives a clear engine setup error; retry after network returns succeeds.
- [ ] Removing the installed engine is detected and repaired or clearly reported.
- [ ] Modifying the installed engine is detected before execution.
- [ ] Antivirus blocking the engine download gives a useful, non-sensitive error.
- [ ] Uninstall asks whether to keep or remove history/settings; no unexpected files remain.
- [ ] Upgrade over an older version preserves data and works.
- [ ] Non-ASCII OS username works.
- [ ] Install and download paths containing spaces work.
- [ ] 125% display scaling is usable.
- [ ] High-contrast / system accessibility settings remain usable.
- [ ] Keyboard-only operation works and focus is visible.
- [ ] Invalid and non-network URL schemes are rejected.
- [ ] Large, malformed, and traversal-containing archive fixtures are rejected.

Status: not run in this environment. This container has no Rust toolchain and no desktop session.
