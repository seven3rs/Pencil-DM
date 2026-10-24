import { describe, expect, it } from "vitest";
import { computeEta, countByFilter, enrich, matchesFilter, smoothSpeed } from "./downloads";
import type { RawDownload } from "./types";

const raw = (over: Partial<RawDownload> = {}): RawDownload => ({
  gid: "g1",
  status: "active",
  name: "file.bin",
  path: "/tmp/file.bin",
  total: 1000,
  completed: 0,
  speed: 100,
  error: null,
  ...over,
});

describe("smoothSpeed", () => {
  it("starts from the first sample", () => {
    expect(smoothSpeed(undefined, 200)).toBe(200);
    expect(smoothSpeed(0, 200)).toBe(200);
  });
  it("blends new samples into the average", () => {
    expect(smoothSpeed(100, 200)).toBeCloseTo(130);
  });
});

describe("computeEta", () => {
  it("divides remaining bytes by speed", () => {
    expect(computeEta(1000, 400, 100)).toBe(6);
  });
  it("is unknown without size, speed or when finished", () => {
    expect(computeEta(0, 0, 100)).toBeNull();
    expect(computeEta(1000, 0, 0)).toBeNull();
    expect(computeEta(1000, 1000, 100)).toBeNull();
  });
});

describe("filters", () => {
  it("maps statuses to sidebar groups", () => {
    expect(matchesFilter("waiting", "downloading")).toBe(true);
    expect(matchesFilter("complete", "completed")).toBe(true);
    expect(matchesFilter("error", "failed")).toBe(true);
    expect(matchesFilter("paused", "downloading")).toBe(false);
    expect(matchesFilter("anything", "all")).toBe(true);
  });
  it("counts each group", () => {
    const counts = countByFilter([
      { status: "active" },
      { status: "waiting" },
      { status: "paused" },
      { status: "complete" },
      { status: "error" },
      { status: "removed" },
    ]);
    expect(counts).toEqual({ all: 6, downloading: 2, completed: 1, paused: 1, failed: 1 });
  });
});

describe("enrich", () => {
  it("computes percent, smoothed speed and ETA for active downloads", () => {
    const speeds = new Map<string, number>();
    const [row] = enrich([raw({ completed: 500, speed: 100 })], speeds);
    expect(row.percent).toBe(50);
    expect(row.smoothSpeed).toBe(100);
    expect(row.eta).toBe(5);
  });
  it("carries the speed average between polls and forgets removed downloads", () => {
    const speeds = new Map<string, number>();
    enrich([raw({ speed: 100 })], speeds);
    const [row] = enrich([raw({ speed: 200 })], speeds);
    expect(row.smoothSpeed).toBeCloseTo(130);
    enrich([], speeds);
    expect(speeds.size).toBe(0);
  });
  it("shows no speed or ETA for paused downloads and 100% for complete ones", () => {
    const speeds = new Map<string, number>();
    const [paused, done] = enrich(
      [raw({ status: "paused", completed: 10 }), raw({ gid: "g2", status: "complete", completed: 1000 })],
      speeds,
    );
    expect(paused.smoothSpeed).toBe(0);
    expect(paused.eta).toBeNull();
    expect(done.percent).toBe(100);
  });
  it("leaves percent unknown when the size is unknown", () => {
    const [row] = enrich([raw({ total: 0 })], new Map());
    expect(row.percent).toBeNull();
  });
});
