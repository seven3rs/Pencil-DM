import { describe, expect, it } from "vitest";
import { scrollToReveal, visibleRange } from "./virtual";

describe("visibleRange", () => {
  it("renders only a window of a 5,000 row list", () => {
    const r = visibleRange(0, 440, 44, 5000, 6);
    expect(r).toEqual({ start: 0, end: 16 });
    const mid = visibleRange(44 * 2500, 440, 44, 5000, 6);
    expect(mid.end - mid.start).toBeLessThan(30);
    expect(mid.start).toBe(2494);
  });
  it("clamps at both ends and handles empty lists", () => {
    expect(visibleRange(10_000_000, 440, 44, 100).end).toBe(100);
    expect(visibleRange(-50, 440, 44, 100).start).toBe(0);
    expect(visibleRange(0, 440, 44, 0)).toEqual({ start: 0, end: 0 });
  });
});

describe("scrollToReveal", () => {
  it("does nothing when the row is visible", () => {
    expect(scrollToReveal(3, 0, 440, 44)).toBe(0);
  });
  it("scrolls up or down by the minimum amount", () => {
    expect(scrollToReveal(1, 200, 440, 44)).toBe(44);
    expect(scrollToReveal(20, 0, 440, 44)).toBe(20 * 44 + 44 - 440);
  });
});
