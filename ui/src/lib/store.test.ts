import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Api } from "./api";
import { DownloadStore, POLL_INTERVAL_MS, type StoreEnv } from "./store.svelte";
import type { EngineStatus, RawDownload } from "./types";

const row = (gid: string, status = "active"): RawDownload => ({
  gid,
  status,
  name: `${gid}.bin`,
  path: `/tmp/${gid}.bin`,
  total: 1000,
  completed: 100,
  speed: 50,
  error: null,
});

function setup() {
  let visible = true;
  let visibilityHandler: () => void = () => {};
  let statusHandler: (s: EngineStatus) => void = () => {};
  const downloads: RawDownload[] = [row("a"), row("b", "paused")];
  const api = {
    listDownloads: vi.fn(async () => downloads.map((d) => ({ ...d }))),
    addDownload: vi.fn(async (_url: string) => "gid"),
    pauseDownload: vi.fn(async (_gid: string) => {}),
    resumeDownload: vi.fn(async (_gid: string) => {}),
    removeDownload: vi.fn(async (_gid: string) => {}),
    recheckEngine: vi.fn(async () => {}),
    setEnginePath: vi.fn(async (_path: string) => {}),
    getEngineStatus: vi.fn(async (): Promise<EngineStatus> => ({ state: "starting", detail: "", command: "" })),
    onEngineStatus: vi.fn(async (h: (s: EngineStatus) => void) => {
      statusHandler = h;
      return () => {};
    }),
    onEngineProgress: vi.fn(async () => () => {}),
  } satisfies Api;
  const env: StoreEnv = {
    isVisible: () => visible,
    onVisibilityChange(h) {
      visibilityHandler = h;
      return () => {};
    },
  };
  const store = new DownloadStore(api, env);
  return {
    api,
    store,
    downloads,
    setVisible(v: boolean) {
      visible = v;
      visibilityHandler();
    },
    emit: (s: Partial<EngineStatus> & { state: EngineStatus["state"] }) =>
      statusHandler({ detail: "", command: "", ...s }),
  };
}

const flush = async () => {
  await vi.advanceTimersByTimeAsync(0);
};

describe("DownloadStore", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("does not poll until the engine is ready", async () => {
    const t = setup();
    await t.store.start();
    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3);
    expect(t.api.listDownloads).not.toHaveBeenCalled();
    t.emit({ state: "ready" });
    await flush();
    expect(t.api.listDownloads).toHaveBeenCalledTimes(1);
    t.store.stop();
  });

  it("polls once per second while visible", async () => {
    const t = setup();
    await t.store.start();
    t.emit({ state: "ready" });
    await flush();
    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3);
    expect(t.api.listDownloads).toHaveBeenCalledTimes(4);
    expect(t.store.items).toHaveLength(2);
    t.store.stop();
  });

  it("stops polling while hidden and refreshes right away when shown again", async () => {
    const t = setup();
    await t.store.start();
    t.emit({ state: "ready" });
    await flush();
    t.setVisible(false);
    const calls = t.api.listDownloads.mock.calls.length;
    await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 5);
    expect(t.api.listDownloads).toHaveBeenCalledTimes(calls);
    t.setVisible(true);
    await flush();
    expect(t.api.listDownloads).toHaveBeenCalledTimes(calls + 1);
    t.store.stop();
  });

  it("filters, counts and tracks the selection", async () => {
    const t = setup();
    await t.store.start();
    t.emit({ state: "ready" });
    await flush();
    t.store.setFilter("paused");
    expect(t.store.visible.map((d) => d.gid)).toEqual(["b"]);
    expect(t.store.counts.all).toBe(2);
    t.store.select("a");
    expect(t.store.selected?.gid).toBe("a");
    t.downloads.shift();
    await t.store.refresh();
    expect(t.store.selectedGid).toBeNull();
    t.store.stop();
  });

  it("toggle pauses running downloads and resumes paused ones", async () => {
    const t = setup();
    await t.store.start();
    t.emit({ state: "ready" });
    await flush();
    await t.store.toggle("a");
    await t.store.toggle("b");
    expect(t.api.pauseDownload).toHaveBeenCalledWith("a");
    expect(t.api.resumeDownload).toHaveBeenCalledWith("b");
    t.store.stop();
  });

  it("remove clears the selection and reports failures as text", async () => {
    const t = setup();
    await t.store.start();
    t.emit({ state: "ready" });
    await flush();
    t.store.select("a");
    await t.store.remove("a");
    expect(t.store.selectedGid).toBeNull();
    t.api.pauseDownload.mockRejectedValueOnce("engine busy");
    await t.store.pause("b");
    expect(t.store.notice).toEqual({ text: "engine busy", isError: true });
    t.store.stop();
  });

  it("add resolves to an error message instead of throwing", async () => {
    const t = setup();
    expect(await t.store.add("https://a.com/x")).toBeNull();
    t.api.addDownload.mockRejectedValueOnce("bad link");
    expect(await t.store.add("https://a.com/y")).toBe("bad link");
  });

  it("shows engine errors and the missing-engine command", async () => {
    const t = setup();
    await t.store.start();
    t.emit({ state: "missing", detail: "Install it", command: "brew install aria2" });
    expect(t.store.engine.command).toBe("brew install aria2");
    expect(t.store.notice.text).toBe("");
    t.emit({ state: "error", detail: "It broke" });
    expect(t.store.notice).toEqual({ text: "It broke", isError: true });
    t.store.stop();
  });
});
