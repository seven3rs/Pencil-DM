import { describe, expect, it } from "vitest";
import { installInstructions, lastSeenLabel, pairingRemaining, visibleBrowsers } from "./browser";
import type { BrowserInfo } from "./api";

describe("lastSeenLabel", () => {
  const now = 1_000_000_000_000;
  it("handles never, recent and old contact", () => {
    expect(lastSeenLabel(null, now)).toBe("not seen yet");
    expect(lastSeenLabel(now / 1000 - 3, now)).toBe("just now");
    expect(lastSeenLabel(now / 1000 - 30, now)).toBe("30 s ago");
    expect(lastSeenLabel(now / 1000 - 5 * 60, now)).toBe("5 min ago");
    expect(lastSeenLabel(now / 1000 - 3 * 3600, now)).toBe("3 h ago");
    expect(lastSeenLabel(now / 1000 - 5 * 86400, now)).toBe("5 days ago");
  });
  it("never goes negative when clocks disagree", () => {
    expect(lastSeenLabel(now / 1000 + 50, now)).toBe("just now");
  });
});

describe("pairingRemaining", () => {
  it("counts down and expires", () => {
    expect(pairingRemaining(300_000, 0)).toBe("5:00");
    expect(pairingRemaining(300_000, 241_000)).toBe("0:59");
    expect(pairingRemaining(300_000, 300_000)).toBeNull();
    expect(pairingRemaining(300_000, 400_000)).toBeNull();
  });
});

describe("browser lists and instructions", () => {
  const all: BrowserInfo[] = [
    { id: "chrome", name: "Google Chrome", family: "chromium", installed: true },
    { id: "edge", name: "Microsoft Edge", family: "chromium", installed: false },
  ];
  it("hides browsers that are not installed unless asked", () => {
    expect(visibleBrowsers(all, false).map((b) => b.id)).toEqual(["chrome"]);
    expect(visibleBrowsers(all, true)).toHaveLength(2);
  });
  it("warns that Firefox temporary add-ons do not survive a restart", () => {
    expect(installInstructions("firefox").join(" ")).toMatch(/restart/i);
    expect(installInstructions("chromium").join(" ")).toMatch(/Load unpacked/);
  });
});
