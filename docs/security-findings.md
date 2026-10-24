# Security findings

This is an initial tracking document, not a claim that an independent audit has been completed.

| ID | Severity | Finding | Status |
|---|---|---|---|
| PDM-001 | Medium | Browser integration, pairing, and local endpoint are not implemented. | Accepted as out of scope for the current build; do not advertise browser capture until implemented and tested. |
| PDM-002 | Medium | Auto-updater signing and Windows/macOS code signing are not configured. | Open; release must remain a draft until release hardening is complete. |
| PDM-003 | Medium | No end-to-end desktop download smoke test in CI. | Open; requires a desktop-capable Linux runner and Tauri WebDriver setup. |
| PDM-004 | Low | Current UI uses the global Tauri bridge. | Open; migrate to `@tauri-apps/api` with `withGlobalTauri` disabled. |
| PDM-005 | Medium | No fuzz/property suite for archive extraction and filename handling. | Open. |

No issue in this table is represented as fixed unless the status explicitly says so.
