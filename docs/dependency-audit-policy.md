# Dependency Audit and Supply Chain Policy

This document defines the dependency review, automated audit, and verification policy for Pencil Download Manager across Rust workspace crates, npm packages, and upstream engine distribution archives.

---

## 1. Objectives

- Ensure all direct and transitive third-party dependencies are continuously scanned for known Common Vulnerabilities and Exposures (CVEs) and security advisories.
- Prevent supply-chain compromise, code injection, and prototype pollution in desktop, frontend, and build toolchains.
- Validate that platform-specific archive layouts (notably Windows distribution archives) are verified against embedded manifests under authentic runner conditions.

---

## 2. Rust Dependencies Policy

### 2.1 Audit Tooling and Advisory Database
- CI enforces `cargo audit` in the `dependency-audit` workflow job, querying the authoritative [RustSec Advisory Database](https://rustsec.org/).
- The scan inspects `Cargo.lock` across the entire workspace (`pdm-engine` and `pencil-download-manager`).

### 2.2 Severity Thresholds and Gating
- **Critical / High Severity:** Immediate build failure. Pull requests containing Critical or High advisories cannot be merged.
- **Medium / Low Severity:** Must be triaged within 14 days of disclosure. If an upstream patch is available, dependencies must be upgraded.
- **Unmaintained Crate Warnings:** Triaged to ensure alternatives or forks are adopted if security maintenance is required.

### 2.3 Lockfile Hygiene
- `Cargo.lock` is version-controlled and pinned.
- CI builds use deterministic resolution (`--locked` where supported).

---

## 3. npm / Frontend Dependencies Policy

### 3.1 Audit Tooling
- CI enforces `npm audit --audit-level=high` in the `dependency-audit` workflow job.
- Clean `npm ci` installs are executed against the version-controlled `package-lock.json`.

### 3.2 Severity Thresholds and Remediation
- **Critical / High Severity:** Blocks merge and production packaging.
- **Remediation Order:**
  1. Update direct dependencies in `package.json` to patched versions.
  2. For transitive dependencies with lagging top-level parents, define explicit, audited `overrides` in `package.json`.
  3. Re-run `npm run check`, `npm test`, and `npm run build` to verify zero regression.

---

## 4. Platform Archive Verification Policy

### 4.1 Windows Real Archive Layout Test
- Unit test suites run offline in CI matrix across Linux, macOS, and Windows runners using generated fixtures.
- To prevent regressions in handling authentic upstream archive structures (such as nested folder prefixes like `aria2-<version>-win-64bit-build1/aria2c.exe`), CI includes a dedicated job on `windows-latest`:
  - Downloads the actual release artifact referenced in `engine-manifest.json` over HTTPS.
  - Verifies the download SHA-256 against the embedded manifest.
  - Extracts the real archive layout, asserting traversal-safety and safe entry handling.
  - Verifies the binary's cryptographic digest and tests idempotent re-verification.
