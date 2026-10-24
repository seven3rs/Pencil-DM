import { describe, expect, it } from "vitest";
import { formatBytes, formatEta, formatPercent, formatSpeed, statusLabel } from "./format";

describe("formatBytes", () => {
  it("shows a dash for zero, negative and invalid values", () => {
    expect(formatBytes(0)).toBe("—");
    expect(formatBytes(-5)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });
  it("scales through the units", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 ** 2)).toBe("5.0 MB");
    expect(formatBytes(3 * 1024 ** 3)).toBe("3.0 GB");
  });
  it("stops at terabytes", () => {
    expect(formatBytes(2048 * 1024 ** 4)).toBe("2048.0 TB");
  });
});

describe("formatSpeed", () => {
  it("adds a per-second suffix", () => {
    expect(formatSpeed(2 * 1024 ** 2)).toBe("2.0 MB/s");
  });
  it("shows a dash when idle", () => {
    expect(formatSpeed(0)).toBe("—");
  });
});

describe("formatEta", () => {
  it("handles unknown values", () => {
    expect(formatEta(null)).toBe("—");
    expect(formatEta(Number.POSITIVE_INFINITY)).toBe("—");
    expect(formatEta(-1)).toBe("—");
  });
  it("formats seconds, minutes, hours and days", () => {
    expect(formatEta(45)).toBe("45s");
    expect(formatEta(185)).toBe("3m 05s");
    expect(formatEta(2 * 3600 + 3 * 60)).toBe("2h 03m");
    expect(formatEta(28 * 3600)).toBe("1d 4h");
  });
});

describe("formatPercent", () => {
  it("formats with one decimal until complete", () => {
    expect(formatPercent(null)).toBe("—");
    expect(formatPercent(42.345)).toBe("42.3%");
    expect(formatPercent(100)).toBe("100%");
  });
});

describe("statusLabel", () => {
  it("maps engine statuses to friendly names", () => {
    expect(statusLabel("active")).toBe("Downloading");
    expect(statusLabel("waiting")).toBe("Queued");
    expect(statusLabel("complete")).toBe("Completed");
    expect(statusLabel("error")).toBe("Failed");
  });
  it("passes unknown values through as text", () => {
    expect(statusLabel("<b>odd</b>")).toBe("<b>odd</b>");
    expect(statusLabel("")).toBe("Unknown");
  });
});
