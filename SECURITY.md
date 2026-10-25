# Security policy

Pencil Download Manager is an early-stage project. No stable release exists yet, so no released version is formally supported. Security reports against the latest default branch (`main`) are welcome.

## Reporting a vulnerability

Please report suspected vulnerabilities **privately**:

1. Open the repository's **Security** tab and choose **Report a vulnerability** (GitHub private security advisories).
2. If that option is unavailable, contact the repository owner privately through the hosting provider. This repository does not yet have a dedicated security email address; one will be listed here when it exists.

Please do not open a public issue, pull request, or discussion about a suspected vulnerability, and do not publish details until a fix or a coordinated disclosure date has been agreed.

Include, where you can:

- the affected version or commit, operating system, and how you installed the app;
- steps to reproduce, and what you expected to happen versus what happened;
- the impact as you understand it.

**Do not include** personal data, credentials, tokens, or private download URLs in reports. Replace them with placeholders.

## What to expect

This is a small project, so these are targets, not guarantees:

- acknowledgement within [7] days;
- an initial assessment within [14] days;
- credit in the release notes or advisory if you want it.

## Scope

In scope:

- the desktop app (`src-tauri`, `ui`, `crates/engine`);
- engine installation and verification: the manifest, hash checks, archive extraction, and repair on launch;
- the local control channel to the engine (loopback JSON-RPC with a per-launch secret);
- handling of network-provided names and errors, which must always be rendered as text, never HTML.

Out of scope for this repository:

- vulnerabilities in **aria2** itself. Please report those to the [aria2 project](https://github.com/aria2/aria2) first. We will update the pinned engine version in `engine-manifest.json` once a fixed release is available;
- vulnerabilities in other third-party dependencies, unless our use of them creates the problem;
- attacks that require an already-compromised machine or administrator access.

## Related documents

See [the threat model](docs/threat-model.md) and [security findings](docs/security-findings.md).

## Good-faith research

We will not pursue action against people who act in good faith: research only on your own systems and data, no disruption of other users, and private reporting as described above.

## Supply chain

The download engine (aria2) is pinned to an exact version, URL and SHA-256 in `engine-manifest.json`. Release builds fetch or build only those pinned inputs and fail on any hash mismatch; there are no "latest" lookups at build time. The app re-checks the bundled engine's hash at every launch and refuses to run a modified copy.
