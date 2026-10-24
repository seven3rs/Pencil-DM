# Third-party notices

Pencil Download Manager uses a separate download-engine program that is
downloaded and verified at install time (it is not part of this repository or
linked into the app).

- **aria2** - GNU GPL v2 (or later), https://aria2.github.io/
  Source code: https://github.com/aria2/aria2
  The exact release and its checksums are pinned in `engine-manifest.json`.

Because the engine runs as its own process, this project's code is not a
derivative work of it and keeps its own license. Keep this notice (or an
equivalent entry in the app's About screen) if you redistribute the engine
binary yourself, for example in an offline installer.
