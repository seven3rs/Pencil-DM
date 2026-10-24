import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { EngineStatus, RawDownload } from "./types";

/** Everything the store needs from the backend, so tests can swap in a fake. */
export interface Api {
  listDownloads(): Promise<RawDownload[]>;
  addDownload(url: string): Promise<string>;
  pauseDownload(gid: string): Promise<void>;
  resumeDownload(gid: string): Promise<void>;
  removeDownload(gid: string): Promise<void>;
  recheckEngine(): Promise<void>;
  setEnginePath(path: string): Promise<void>;
  onEngineStatus(handler: (status: EngineStatus) => void): Promise<() => void>;
  onEngineProgress(handler: (done: number, total: number | null) => void): Promise<() => void>;
}

export const tauriApi: Api = {
  listDownloads: () => invoke<RawDownload[]>("list_downloads"),
  addDownload: (url) => invoke<string>("add_download", { url }),
  pauseDownload: (gid) => invoke("pause_download", { gid }),
  resumeDownload: (gid) => invoke("resume_download", { gid }),
  removeDownload: (gid) => invoke("remove_download", { gid }),
  recheckEngine: () => invoke("recheck_engine"),
  setEnginePath: (path) => invoke("set_engine_path", { path }),
  onEngineStatus: (handler) =>
    listen<{ state: EngineStatus["state"]; detail?: string; command?: string }>(
      "engine-status",
      (ev) =>
        handler({
          state: ev.payload.state,
          detail: ev.payload.detail ?? "",
          command: ev.payload.command ?? "",
        }),
    ),
  onEngineProgress: (handler) =>
    listen<{ done: number; total: number | null }>("engine-progress", (ev) =>
      handler(ev.payload.done, ev.payload.total),
    ),
};
