/** Mirrors `DownloadInfo` in crates/engine (serialized by the Tauri commands). */
export interface RawDownload {
  gid: string;
  status: string;
  name: string;
  path: string;
  total: number;
  completed: number;
  speed: number;
  error: string | null;
}

/** A download plus values the client computes (smoothed speed, ETA). */
export interface Download extends RawDownload {
  smoothSpeed: number;
  /** Seconds remaining, or null when unknown. */
  eta: number | null;
  /** 0-100, or null when the total size is unknown. */
  percent: number | null;
}

export type EngineState = "starting" | "ready" | "missing" | "error";

export interface EngineStatus {
  state: EngineState;
  detail: string;
  command: string;
}

export type FilterId = "all" | "downloading" | "completed" | "paused" | "failed";
