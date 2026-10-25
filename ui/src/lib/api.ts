import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { FileAllocationMode } from "./allocation";
import type { EngineStatus, RawDownload } from "./types";

/** Everything the store needs from the backend, so tests can swap in a fake. */
export interface Api {
  listDownloads(): Promise<RawDownload[]>;
  /** `fileAllocation` overrides the saved default for this download only. */
  addDownload(url: string, fileAllocation?: FileAllocationMode): Promise<string>;
  pauseDownload(gid: string): Promise<void>;
  resumeDownload(gid: string): Promise<void>;
  removeDownload(gid: string): Promise<void>;
  recheckEngine(): Promise<void>;
  setEnginePath(path: string): Promise<void>;
  /** Current engine status, for recovering from a missed `engine-status` event on startup. */
  getEngineStatus(): Promise<EngineStatus>;
  onEngineStatus(handler: (status: EngineStatus) => void): Promise<() => void>;
  onEngineProgress(handler: (done: number, total: number | null) => void): Promise<() => void>;
}

export interface Aria2About { version: string; tag: string; source_url: string; license: string; }

export interface ScheduleRule { id: string; name: string; days: number[]; start_time: string; stop_time: string; timezone: string; speed_limit: number | null; enabled: boolean; }

export interface BrowserStatus { paired: boolean; origin: string | null; last_seen: number | null; port: number; discovery_ok: boolean; discovery_error: string | null; }
export interface BrowserInfo { id: string; name: string; family: "chromium" | "firefox"; installed: boolean; }

type ExtendedApi = Api & {
  createBrowserPairingCode(): Promise<string>;
  getBrowserStatus(): Promise<BrowserStatus>;
  disconnectBrowser(): Promise<void>;
  listBrowsers(): Promise<BrowserInfo[]>;
  prepareExtension(family: string): Promise<string>;
  openBrowserExtensions(browser: string): Promise<string>;
  revealExtensionFolder(family: string): Promise<void>;
  getSettings(): Promise<[string, string][]>;
  setSpeedLimiter(enabled: boolean, bytesPerSecond: number): Promise<void>;
  setDownloadSpeedLimit(gid: string, bytesPerSecond: number | null): Promise<void>;
  getScheduleRules(): Promise<ScheduleRule[]>;
  saveScheduleRules(rules: ScheduleRule[]): Promise<void>;
  verifyChecksum(path: string, expected: string): Promise<boolean>;
  setDownloadChecksum(gid: string, expected: string): Promise<void>;
  setPostDownloadAction(gid: string, action: string): Promise<void>;
  setPostDownloadProgram(gid: string, path: string, confirmed: boolean): Promise<void>;
  cancelPowerAction(): Promise<void>;
  setSetting(key: string, value: string): Promise<void>;
  getPendingUrls(): Promise<string[]>;
  pauseAll(): Promise<void>;
  resumeAll(): Promise<void>;
  setAutostartEnabled(enabled: boolean): Promise<void>;
  getAutostartEnabled(): Promise<boolean>;
  writeClipboard(text: string): Promise<void>;
  copyDownloadLink(gid: string): Promise<void>;
  openDownloadFile(gid: string): Promise<void>;
  revealDownload(gid: string): Promise<void>;
  deleteDownloadFile(gid: string, confirmed: boolean): Promise<void>;
  readDroppedLinks(paths: string[]): Promise<string[]>;
  openThirdPartyNotices(): Promise<void>;
  diagnosticsReport(): Promise<string>;
  aria2About(): Promise<Aria2About>;
  openAria2Source(): Promise<void>;
  deleteAllAppData(confirmed: boolean): Promise<void>;
};

export const tauriApi: ExtendedApi = {
  createBrowserPairingCode: () => invoke<string>("create_browser_pairing_code"),
  getBrowserStatus: () => invoke<BrowserStatus>("get_browser_status"),
  disconnectBrowser: () => invoke("disconnect_browser"),
  listBrowsers: () => invoke<BrowserInfo[]>("list_browsers"),
  prepareExtension: (family) => invoke<string>("prepare_extension", { family }),
  openBrowserExtensions: (browser) => invoke<string>("open_browser_extensions", { browser }),
  revealExtensionFolder: (family) => invoke("reveal_extension_folder", { family }),
  listDownloads: () => invoke<RawDownload[]>("list_downloads"),
  addDownload: (url, fileAllocation) => invoke<string>("add_download", { url, fileAllocation }),
  pauseDownload: (gid) => invoke("pause_download", { gid }),
  resumeDownload: (gid) => invoke("resume_download", { gid }),
  removeDownload: (gid) => invoke("remove_download", { gid }),
  recheckEngine: () => invoke("recheck_engine"),
  setEnginePath: (path) => invoke("set_engine_path", { path }),
  getEngineStatus: () => invoke<EngineStatus>("get_engine_status"),
  getSettings: () => invoke<[string, string][]>("get_settings"),
  setSpeedLimiter: (enabled, bytesPerSecond) => invoke("set_speed_limiter", { enabled, bytesPerSecond }),
  setDownloadSpeedLimit: (gid, bytesPerSecond) => invoke("set_download_speed_limit", { gid, bytesPerSecond }),
  getScheduleRules: () => invoke<ScheduleRule[]>("get_schedule_rules"),
  saveScheduleRules: (rules) => invoke("save_schedule_rules", { rules }),
  verifyChecksum: (path, expected) => invoke<boolean>("verify_checksum", { path, expected }),
  setDownloadChecksum: (gid, expected) => invoke("set_download_checksum", { gid, expected }),
  setPostDownloadAction: (gid, action) => invoke("set_post_download_action", { gid, action }),
  setPostDownloadProgram: (gid, path, confirmed) => invoke("set_post_download_program", { gid, path, confirmed }),
  cancelPowerAction: () => invoke("cancel_power_action"),
  setSetting: (key, value) => invoke("set_setting", { key, value }),
  getPendingUrls: () => invoke<string[]>("get_pending_urls"),
  pauseAll: () => invoke("pause_all"),
  resumeAll: () => invoke("resume_all"),
  setAutostartEnabled: (enabled) => invoke("set_autostart_enabled", { enabled }),
  getAutostartEnabled: () => invoke<boolean>("get_autostart_enabled"),
  writeClipboard: (text) => invoke("write_clipboard", { text }),
  copyDownloadLink: (gid) => invoke("copy_download_link", { gid }),
  openDownloadFile: (gid) => invoke("open_download_file", { gid }),
  revealDownload: (gid) => invoke("reveal_download", { gid }),
  deleteDownloadFile: (gid, confirmed) => invoke("delete_download_file", { gid, confirmed }),
  readDroppedLinks: (paths) => invoke<string[]>("read_dropped_links", { paths }),
  openThirdPartyNotices: () => invoke("open_third_party_notices"),
  diagnosticsReport: () => invoke<string>("diagnostics_report"),
  aria2About: () => invoke<Aria2About>("aria2_about"),
  openAria2Source: () => invoke("open_aria2_source"),
  deleteAllAppData: (confirmed) => invoke("delete_all_app_data", { confirmed }),
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
