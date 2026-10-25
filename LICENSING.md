# Licensing

Pencil Download Manager follows the same overall scheme as Audacity: the **code is free software under the GNU General Public License**, while the **name and logo are protected as trademarks** under a separate policy.

Copyright (C) [YEAR] [YOUR NAME OR ORGANIZATION] and the Pencil Download Manager contributors.

## What is covered by what

| Component | License |
|---|---|
| Source code: `crates/`, `src-tauri/`, `ui/`, `scripts/`, build and CI files | **GPL-3.0-or-later**, full text in [LICENSE](LICENSE) |
| Documentation: `README.md`, `docs/` | **CC BY 4.0**, <https://creativecommons.org/licenses/by/4.0/> |
| The name "Pencil Download Manager" (as a whole) and the logo/branding files in `[branding/ path]` | **Not licensed under the GPL.** Trademark and branding rights are reserved. See [TRADEMARK.md](TRADEMARK.md) |
| Third-party dependencies (Rust crates, npm packages, Tauri) | Their own licenses, listed in `THIRD_PARTY_NOTICES.md` |
| aria2 (the download engine, run as a separate program) | Its own license (GPL-2.0-or-later with an OpenSSL exception); it is not part of this project's code. See below |

## The GPL part

You may use, study, modify, and redistribute the source code under the terms of the GNU General Public License, version 3, or (at your option) any later version. Anyone who distributes this software or a modified version must make the corresponding source available under the same license.

## Additional term: trademarks (GPLv3 section 7(e))

As permitted by section 7(e) of the GPLv3, no rights are granted under trademark law for use of the trade names, trademarks, or service marks of this project, including the name "Pencil Download Manager" and its logo. This term does not limit any right the GPL gives you to copy, modify, and distribute the code. It only means that modified versions must not use the project's name or logo unless [TRADEMARK.md](TRADEMARK.md) allows it.

## Source file headers

Add this to every new source file, using the comment style of the language:

```
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) [YEAR] [YOUR NAME OR ORGANIZATION] and the Pencil Download Manager contributors
```

## Contributions

Contributions are accepted under the same license as the project (GPL-3.0-or-later for code, CC BY 4.0 for docs). Contributors certify their right to submit the work by signing off their commits (`git commit -s`, the Developer Certificate of Origin, <https://developercertificate.org/>).

## Third-party code and license compatibility

Keep `THIRD_PARTY_NOTICES.md` up to date, for example with `cargo about` for Rust crates and `license-checker` for npm packages.

Every dependency must be compatible with GPL-3.0-or-later:

- **Compatible:** MIT, BSD-2/3-Clause, ISC, Zlib, Apache-2.0, MPL-2.0, Unlicense/CC0, GPL-3.0, LGPL, and AGPL-3.0.
- **Not compatible:** GPL-2.0-only, proprietary or source-available licenses, and licenses with non-commercial or field-of-use restrictions (for example CC BY-NC, SSPL).

The Tauri, Svelte, and typical Rust/npm ecosystem licenses (MIT and Apache-2.0) are all compatible.

## aria2 (download engine)

Pencil Download Manager uses [aria2](https://aria2.github.io/) as its download engine. aria2 is a separate program with its own authors and its own license (GPL-2.0-or-later with an OpenSSL linking exception; see the `COPYING` file of the aria2 release you use). It is not part of this project, and this project's license does not apply to it.

- The app starts aria2 as a separate process and talks to it only over local JSON-RPC. It does not link against aria2's code (`libaria2`) and must not do so without a license review.
- aria2 1.37.0 (upstream tag `release-1.37.0`) is bundled inside the Windows and macOS installers, and is downloaded from the pinned upstream release (see `engine-manifest.json`) or provided by the system package on Linux. The Linux `.deb` and `.rpm` packages do not contain aria2; they depend on the system `aria2` package. For every release we attach the exact source tarball used, [`aria2-1.37.0.tar.xz`](https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0.tar.xz), together with aria2's `COPYING` file, to the GitHub release; the same files are the source offer for the bundled binaries. The installers also carry aria2's license text under `licenses/aria2/`.
- "aria2" is the name of the upstream project. This project is not affiliated with or endorsed by the aria2 developers.

## License history

Earlier commits of this repository, up to and including `[LAST MIT COMMIT]`, were published under the MIT License. Copies of that earlier code that people already have remain available under MIT. From `[FIRST GPL COMMIT]` onward, the project is licensed under GPL-3.0-or-later as described above. Any code contributed by others under the MIT License keeps its MIT notice.
