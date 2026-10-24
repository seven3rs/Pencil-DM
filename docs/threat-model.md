# Threat model

## Assets and boundaries

Assets include the embedded engine hashes, downloaded files, local download history, engine RPC secret, browser pairing credentials (if browser integration is added), and update signing keys. Trust boundaries are the public network, downloaded archives, the child-process RPC on loopback, the UI/webview, browser extensions, and release infrastructure.

## Current controls and gaps

| Area | Existing control | Remaining risk / required follow-up |
|---|---|---|
| Engine installer | HTTPS URLs, archive and binary SHA-256 checks against compile-time manifest, size caps, zip path checks, symlink rejection | Hashes do not authenticate a compromised source manifest at development time; add collision, duplicate-entry, and fuzz/property tests. |
| Redirects | Installer download redirects are restricted to HTTPS | Test downgrade, redirect loops, and host policy explicitly. |
| Engine launch | Binary hash is checked during managed install/resolve; local child process | Re-hash immediately before spawn and make installed binary non-writable where practical to reduce TOCTOU. |
| RPC | Loopback address, random per-process secret, no proxy for loopback calls | Review secret visibility in process arguments/logs and local process threat model. |
| UI | Network-derived display strings use text nodes/textContent; no `innerHTML` in current UI | Current UI still uses global Tauri bridge; migrate to scoped `@tauri-apps/api` imports and a strict capability allow-list. |
| Browser integration | Not implemented | Pairing, origin validation, rate limits, body limits, scheme validation, and fallback behavior are prerequisites before shipping. |
| Updates/signing | Release workflow currently builds draft releases | Signed updater and platform signing/notarization are not implemented; keep signing keys out of source control. |
| User URLs and file names | RPC validates network URL schemes; archive extraction blocks traversal and symlinks | Add comprehensive filename sanitization and persistent-path validation when history/categories are implemented. |

## Review priorities

1. Finish platform engine sourcing without accepting unverified binaries.
2. Add fuzz/property tests for manifest parsing, archive extraction, and filename sanitization.
3. Add authenticated browser integration only after threat-model review.
4. Add dependency audit policy and a real end-to-end download test in CI.
5. Record accepted risks in `docs/security-findings.md` before a public release.
