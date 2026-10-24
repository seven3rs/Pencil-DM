import type { Download, FilterId, RawDownload } from "./types";

/** Weight of the newest sample in the speed average (higher reacts faster). */
export const SPEED_SMOOTHING = 0.3;

export function isRunning(status: string): boolean {
  return status === "active" || status === "waiting";
}

export function matchesFilter(status: string, filter: FilterId): boolean {
  switch (filter) {
    case "all":
      return true;
    case "downloading":
      return isRunning(status);
    case "completed":
      return status === "complete";
    case "paused":
      return status === "paused";
    case "failed":
      return status === "error";
  }
}

export function countByFilter(items: readonly { status: string }[]): Record<FilterId, number> {
  const counts: Record<FilterId, number> = {
    all: items.length,
    downloading: 0,
    completed: 0,
    paused: 0,
    failed: 0,
  };
  for (const item of items) {
    if (isRunning(item.status)) counts.downloading++;
    else if (item.status === "complete") counts.completed++;
    else if (item.status === "paused") counts.paused++;
    else if (item.status === "error") counts.failed++;
  }
  return counts;
}

export function smoothSpeed(previous: number | undefined, current: number): number {
  if (previous === undefined || previous <= 0) return current;
  return SPEED_SMOOTHING * current + (1 - SPEED_SMOOTHING) * previous;
}

export function computeEta(total: number, completed: number, speed: number): number | null {
  if (total <= 0 || speed < 1 || completed >= total) return null;
  return (total - completed) / speed;
}

/**
 * Turn a fresh engine listing into display rows. `previous` holds the speeds
 * from the last poll (by gid) so the average carries over; it is updated in place.
 */
export function enrich(raw: readonly RawDownload[], previous: Map<string, number>): Download[] {
  const alive = new Set<string>();
  const rows = raw.map((r): Download => {
    alive.add(r.gid);
    const running = r.status === "active";
    const smooth = running ? smoothSpeed(previous.get(r.gid), r.speed) : 0;
    previous.set(r.gid, smooth);
    const percent =
      r.total > 0 ? Math.min(100, Math.max(0, (r.completed / r.total) * 100)) : null;
    return {
      ...r,
      smoothSpeed: smooth,
      eta: running ? computeEta(r.total, r.completed, smooth) : null,
      percent: r.status === "complete" ? 100 : percent,
    };
  });
  for (const gid of previous.keys()) if (!alive.has(gid)) previous.delete(gid);
  return rows;
}
