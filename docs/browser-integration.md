# Browser integration

Pencil uses a loopback HTTP endpoint rather than native messaging. The HTTP endpoint is a better fit for the current cross-platform Tauri app: it does not require registering a separate native-host manifest with Chrome, Edge, and Firefox installers. The endpoint binds only to `127.0.0.1` on an ephemeral port. A loopback discovery listener on `127.0.0.1:17653` returns only that port because browser extensions cannot read the app's private data file. If that discovery port is unavailable, browser integration startup fails rather than binding to another interface.

The app writes `browser-connection.json` in its app data directory with the port, a cryptographically random 32-byte token and, once paired, the extension's origin. The file is reused on the next launch, so an extension that is already paired keeps working after Pencil restarts (the app asks for the same port first and the extension re-discovers it if it changed). Choosing **Disconnect** in the app rotates the token and forgets the origin. On Unix the file is created with mode `0600`; on Windows inherited ACLs are removed and full access is granted only to the current user and SYSTEM. All functional endpoints require `Authorization: Bearer <token>`. Pairing is the sole exception: the user explicitly creates a one-time eight-character code in the app, and the extension exchanges it once within five minutes. Pairing accepts only a `chrome-extension://` or `moz-extension://` origin, saves that exact origin, and invalidates the code. Authenticated requests carrying any other Origin are rejected. No permissive CORS headers are sent.

If the discovery port is already taken by another program, the app still starts; the **Browser** page shows the problem and an extension that already knows the port keeps working.

## Endpoints

- `GET /v1/discover` (discovery port only) — returns `{ "app", "port", "paired" }`.
- `GET /v1/ping` — authenticated connection check.
- `POST /v1/pair` — one-time pairing exchange `{ "code": "…", "origin": "chrome-extension://…" }`; returns token and port.
- `POST /v1/download` — authenticated one-download request.
- `POST /v1/batch` — authenticated array of 1–20 download requests.

Download requests accept `url`, optional `referer`, `cookies`, `user_agent`, `file_name`, and `headers`. Only HTTP and HTTPS URLs are accepted. Bodies are limited to 1 MiB, the endpoint is rate-limited, and field/header values are validated before the engine receives them. Referer, cookies, user-agent and extra headers are passed as per-download aria2 options, not global settings. The extension only cancels an existing browser download after Pencil accepts it; if Pencil is not running, disconnected, or rejects the request, the browser download is left running. Context-menu actions fall back to a normal browser download if forwarding fails.

## Installing the extension

Browsers do not let a desktop app install an extension silently (Chrome 137 also removed `--load-extension`), so Pencil guides the install instead. Open **Browser** in the app's top bar:

1. Choose **Set up Chrome / Edge / Brave / Vivaldi / Firefox**. Pencil copies the extension to a folder that does not move (`<app data>/browser-extension/chromium` or `firefox`), keeps it up to date after app updates, and opens the browser's extensions page.
2. In that page turn on Developer mode and choose **Load unpacked** (Chromium browsers) or **Load Temporary Add-on** (Firefox), then select the folder shown in Pencil.
3. Click **Show pairing code** and enter it in the extension popup. The page shows **Browser connected** as soon as the extension pairs.

The extension manifest pins a key, so the extension ID (and therefore the paired origin) is the same wherever the folder is loaded from. Remove the `key` field before uploading to a browser store, which assigns its own ID.

Firefox only keeps unsigned add-ons until it restarts. A permanent Firefox install needs a signed `.xpi` (AMO unlisted signing), and a one-click install for Chrome and Edge needs a Chrome Web Store / Edge Add-ons listing. Neither is set up yet.

Manual alternatives:

- Chrome / Edge: open `chrome://extensions` or `edge://extensions`, enable Developer mode, choose **Load unpacked**, and select `extension/`.
- Firefox: run `extension/build-firefox.sh`, then create a temporary add-on from `about:debugging#/runtime/this-firefox` and select `dist/firefox-extension/manifest.json`.

The extension's interception toggle and small-file/domain exclusions are configurable in the popup. The `downloads` event only exposes size information when the browser already knows it; unknown-size items are not excluded by the size filter. See [PRIVACY.md](../extension/PRIVACY.md).

## Manual test script

1. Start Pencil and load the extension unpacked. Open **Browser** in Pencil, show the pairing code, enter the one-time code in the extension, and verify the popup says **Connected to Pencil**.
2. Click a direct HTTPS download link. Verify the browser's original download is canceled only after the item appears in Pencil. Turn interception off and verify the same link remains in the browser.
3. Sign into a test site, click a downloadable file that requires the session, and verify it completes in Pencil. Confirm the request includes the page referrer and that the session cookie was forwarded. Do not use real credentials in test logs.
4. Right-click an HTTPS link, image, and media element and choose **Download … with Pencil Download Manager**. Verify each enters Pencil. Stop Pencil and repeat: the browser should perform the download normally.
5. Set a 1 MB ignore threshold and add a test domain to the ignored-domain list. Verify known small files and matching domains remain browser downloads. Unknown-size downloads may still be intercepted.
6. From a local test script, send `GET /v1/ping` without `Authorization`; expect `401`. Send a request with a wrong bearer token; expect `401`. After pairing, send a request with a different `Origin`; expect `403`. Send a body larger than 1 MiB; expect `413`. Try `file:///etc/passwd`; expect `400`.
7. Attempt to reuse the pairing code and then use it after five minutes; both attempts must fail. Verify the local `browser-connection.json` file has mode `600` on Unix.
