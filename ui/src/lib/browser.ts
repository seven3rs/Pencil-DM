import type { BrowserInfo } from "./api";

/** "just now", "12 s ago", "3 min ago"… for the extension's last contact. */
export function lastSeenLabel(lastSeen: number | null, nowMs: number = Date.now()): string {
  if (lastSeen === null) return "not seen yet";
  const seconds = Math.max(0, Math.round(nowMs / 1000 - lastSeen));
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds} s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours} h ago`;
  return `${Math.round(hours / 24)} days ago`;
}

/** Remaining pairing time as m:ss, or null once it has expired. */
export function pairingRemaining(expiresAtMs: number, nowMs: number = Date.now()): string | null {
  const left = Math.ceil((expiresAtMs - nowMs) / 1000);
  if (left <= 0) return null;
  return `${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
}

/** Installed browsers first; others are hidden unless the user asks to see all. */
export function visibleBrowsers(all: BrowserInfo[], showAll: boolean): BrowserInfo[] {
  return showAll ? all : all.filter((browser) => browser.installed);
}

/** What to do in the browser after the extensions page opens. */
export function installInstructions(family: "chromium" | "firefox"): string[] {
  if (family === "firefox") {
    return [
      "In the Firefox tab that opened, click “Load Temporary Add-on…”.",
      "Select the manifest.json file inside the folder shown above.",
      "Firefox removes temporary add-ons when it restarts. Until a signed Firefox build is published, repeat these steps after restarting Firefox.",
    ];
  }
  return [
    "In the extensions page that opened, turn on Developer mode (the toggle at the top right, or bottom left in Edge).",
    "Click “Load unpacked” and choose the folder shown above. You can paste its path with the Copy path button.",
    "Pin the Pencil icon from the puzzle-piece menu, then continue with pairing.",
  ];
}
