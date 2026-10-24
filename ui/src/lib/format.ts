const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** 1536 -> "1.5 KB". Zero or invalid input gives an em dash. */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "—";
  let value = n;
  let i = 0;
  while (value >= 1024 && i < UNITS.length - 1) {
    value /= 1024;
    i++;
  }
  return `${value.toFixed(i === 0 ? 0 : 1)} ${UNITS[i]}`;
}

/** Bytes per second -> "2.0 MB/s". Zero gives an em dash. */
export function formatSpeed(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return "—";
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** Seconds -> "45s", "3m 05s", "2h 03m", "1d 4h". Null gives an em dash. */
export function formatEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) return "—";
  const s = Math.round(seconds);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${String(m % 60).padStart(2, "0")}m`;
  return `${Math.floor(h / 24)}d ${h % 24}h`;
}

export function formatPercent(percent: number | null): string {
  if (percent === null) return "—";
  return `${percent.toFixed(percent >= 100 ? 0 : 1)}%`;
}

/** Friendly text for an engine status. Unknown statuses are shown as plain text. */
export function statusLabel(status: string): string {
  switch (status) {
    case "active":
      return "Downloading";
    case "waiting":
      return "Queued";
    case "paused":
      return "Paused";
    case "complete":
      return "Completed";
    case "error":
      return "Failed";
    case "removed":
      return "Removed";
    default:
      return status || "Unknown";
  }
}
