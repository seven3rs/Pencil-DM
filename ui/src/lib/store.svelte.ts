import type { Api } from "./api";
import { countByFilter, enrich, isRunning, matchesFilter } from "./downloads";
import type { Download, EngineStatus, FilterId } from "./types";

export const POLL_INTERVAL_MS = 1000;

export interface StoreEnv {
  /** True while the window is visible. Injected so tests can control it. */
  isVisible(): boolean;
  onVisibilityChange(handler: () => void): () => void;
}

export const browserEnv: StoreEnv = {
  isVisible: () => document.visibilityState === "visible",
  onVisibilityChange(handler) {
    document.addEventListener("visibilitychange", handler);
    return () => document.removeEventListener("visibilitychange", handler);
  },
};

/**
 * The single source of truth for the UI. Polls `list_downloads` once per second,
 * but only while the engine is ready and the window is visible.
 */
export class DownloadStore {
  // $state.raw: the list is replaced wholesale every poll, so deep proxies would only cost time.
  items = $state.raw<Download[]>([]);
  filter = $state<FilterId>("all");
  selectedGid = $state<string | null>(null);
  engine = $state<EngineStatus>({ state: "starting", detail: "", command: "" });
  preparing = $state<string>("");
  notice = $state<{ text: string; isError: boolean }>({ text: "", isError: false });

  visible = $derived(this.items.filter((d) => matchesFilter(d.status, this.filter)));
  counts = $derived(countByFilter(this.items));
  selected = $derived(this.items.find((d) => d.gid === this.selectedGid) ?? null);
  ready = $derived(this.engine.state === "ready");

  private speeds = new Map<string, number>();
  private timer: ReturnType<typeof setInterval> | null = null;
  private polling = false;
  /** True while the visible error came from a failed poll, so the next good poll can clear it. */
  private pollFailed = false;
  private cleanups: Array<() => void> = [];

  constructor(
    private api: Api,
    private env: StoreEnv = browserEnv,
  ) {}

  async start(): Promise<void> {
    this.cleanups.push(this.env.onVisibilityChange(() => this.syncPolling()));
    const unlisten = await Promise.all([
      this.api.onEngineStatus((status) => this.setEngine(status)),
      this.api.onEngineProgress((done, total) => {
        this.preparing = total ? `Preparing… ${Math.round((done / total) * 100)}%` : "Preparing…";
      }),
    ]);
    this.cleanups.push(...unlisten);
    // The backend starts the engine as soon as the window is created, not
    // when this listener finishes registering, so the "starting"/"ready"/etc.
    // event can already have fired and been lost by this point. Pull the
    // current status directly to recover from that race instead of being
    // stuck showing the initial placeholder forever.
    this.setEngine(await this.api.getEngineStatus());
    this.syncPolling();
  }

  stop(): void {
    this.stopTimer();
    for (const cleanup of this.cleanups.splice(0)) cleanup();
  }

  setEngine(status: EngineStatus): void {
    this.engine = status;
    this.preparing = "";
    if (status.state === "error") this.say(status.detail || "The download engine could not start.", true);
    else this.say("");
    this.syncPolling();
  }

  say(text: string, isError = false): void {
    this.notice = { text, isError };
  }

  private syncPolling(): void {
    const shouldPoll = this.ready && this.env.isVisible();
    if (shouldPoll && !this.timer) {
      void this.refresh();
      this.timer = setInterval(() => void this.refresh(), POLL_INTERVAL_MS);
    } else if (!shouldPoll) {
      this.stopTimer();
    }
  }

  private stopTimer(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  async refresh(): Promise<void> {
    if (!this.ready || this.polling) return;
    this.polling = true;
    try {
      this.items = enrich(await this.api.listDownloads(), this.speeds);
      if (this.pollFailed) {
        this.pollFailed = false;
        this.say("");
      }
      if (this.selectedGid && !this.items.some((d) => d.gid === this.selectedGid)) {
        this.selectedGid = null;
      }
    } catch (e) {
      this.pollFailed = true;
      this.say(String(e), true);
    } finally {
      this.polling = false;
    }
  }

  select(gid: string | null): void {
    this.selectedGid = gid;
  }

  setFilter(filter: FilterId): void {
    this.filter = filter;
  }

  /** Add one link; resolves to an error message, or null on success. */
  async add(url: string): Promise<string | null> {
    try {
      await this.api.addDownload(url);
      return null;
    } catch (e) {
      return String(e);
    }
  }

  async afterAdds(): Promise<void> {
    await this.refresh();
  }

  async pause(gid: string): Promise<void> {
    await this.run(() => this.api.pauseDownload(gid));
  }

  async resume(gid: string): Promise<void> {
    await this.run(() => this.api.resumeDownload(gid));
  }

  async toggle(gid: string): Promise<void> {
    const item = this.items.find((d) => d.gid === gid);
    if (!item) return;
    if (isRunning(item.status)) await this.pause(gid);
    else if (item.status === "paused") await this.resume(gid);
  }

  async remove(gid: string): Promise<void> {
    await this.run(() => this.api.removeDownload(gid));
    if (this.selectedGid === gid) this.selectedGid = null;
  }

  async recheck(): Promise<void> {
    this.setEngine({ state: "starting", detail: "", command: "" });
    try {
      await this.api.recheckEngine();
    } catch (e) {
      this.say(String(e), true);
    }
  }

  async saveEnginePath(path: string): Promise<void> {
    const trimmed = path.trim();
    if (!trimmed) {
      this.say("Enter an absolute executable path.", true);
      return;
    }
    try {
      await this.api.setEnginePath(trimmed);
      this.say("Custom path saved. Checking the download engine…");
      await this.api.recheckEngine();
    } catch (e) {
      this.say(String(e), true);
    }
  }

  private async run(action: () => Promise<void>): Promise<void> {
    try {
      await action();
      await this.refresh();
    } catch (e) {
      this.say(String(e), true);
    }
  }
}
