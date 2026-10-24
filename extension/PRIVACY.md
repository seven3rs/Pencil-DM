# Privacy statement

Pencil Download Manager runs locally. The extension sends selected download URLs, referrers, user-agent values, matching site cookies, and any explicitly supplied headers only to the Pencil app on this computer over loopback. Those cookies may grant access to logged-in content, so the local endpoint requires a one-time pairing code, a secret token, and the paired extension origin. The token is stored in the browser's local extension storage. No download metadata or cookies are sent to a Pencil-operated server or other third party. The extension does not collect analytics or telemetry.

The extension can inspect cookies for the URL being downloaded so authenticated downloads can work. Unpair by clearing the extension's local storage or removing the extension; restart Pencil to rotate the local token.
