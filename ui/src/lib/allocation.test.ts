import { describe, expect, it } from "vitest";
import { ALLOCATION_MODES, DEFAULT_ALLOCATION, parseAllocation } from "./allocation";

describe("file allocation modes", () => {
  it("defaults to falloc", () => {
    expect(DEFAULT_ALLOCATION).toBe("falloc");
    expect(parseAllocation(undefined)).toBe("falloc");
    expect(parseAllocation("")).toBe("falloc");
  });

  it("keeps the three supported values and rejects anything else", () => {
    expect(ALLOCATION_MODES.map((m) => m.value)).toEqual(["falloc", "none", "prealloc"]);
    expect(parseAllocation("none")).toBe("none");
    expect(parseAllocation("prealloc")).toBe("prealloc");
    expect(parseAllocation("trunc")).toBe("falloc");
  });
});
