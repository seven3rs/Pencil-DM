# Desktop integration manual test

Run these checks on each target OS before release. They are not substitutes for the CI build.

## Windows, macOS, and Linux

- Launch the app twice. The second launch should focus the existing window rather than create another window.
- Launch with a URL argument, and launch `pencil://add?url=https%3A%2F%2Fexample.test%2Ffile.zip`. The Add dialog should contain the URL.
- Close the main window. It should hide to the tray by default. Use the tray menu to show/hide the window, pause all downloads, resume all downloads, and quit. Confirm quitting shuts down the engine cleanly.
- In About, confirm clipboard monitoring starts disabled. Copy a URL while disabled and confirm no prompt appears. Enable it, copy a new HTTP(S)/FTP URL, and confirm the prompt offers to add it. Disable it again and confirm no clipboard prompt appears.
- Drag a URL from a browser onto the app and confirm the Add dialog is prefilled. Drop a `.txt` file containing one URL per line and confirm valid links are prefilled.
- Enable “Start with the operating system”, verify the OS startup entry is created, disable it, and verify it is removed.
- Complete a download and use its context menu to open the file, reveal it in the folder, copy its original URL, and delete the file after confirming. Verify canceling deletion preserves the file.
- Copy the diagnostics report and inspect it for app version, OS, engine status, and sanitized setup messages. Confirm it contains no URLs, absolute paths, or file names.
- Open the third-party notices from About.

## Platform coverage record

- Windows: not tested in this development environment.
- macOS: not tested in this development environment.
- Linux: not tested in this development environment.
