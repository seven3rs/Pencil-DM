import { describe, expect, it } from "vitest";
import { parseLinks, validateLink } from "./links";

describe("validateLink", () => {
  it("accepts http, https and ftp", () => {
    expect(validateLink("https://example.com/a.zip")).toBeNull();
    expect(validateLink("http://example.com/a.zip")).toBeNull();
    expect(validateLink("ftp://example.com/a.zip")).toBeNull();
  });
  it("rejects other schemes and garbage", () => {
    expect(validateLink("file:///etc/passwd")).not.toBeNull();
    expect(validateLink("javascript:alert(1)")).not.toBeNull();
    expect(validateLink("not a link")).not.toBeNull();
  });
  it("rejects very long links", () => {
    expect(validateLink("https://a.com/" + "x".repeat(3000))).toMatch(/too long/);
  });
});

describe("parseLinks", () => {
  it("skips blank lines but keeps original line numbers", () => {
    const lines = parseLinks("https://a.com/1\n\n  \nbad\nhttps://a.com/2");
    expect(lines.map((l) => l.line)).toEqual([1, 4, 5]);
    expect(lines.map((l) => l.error === null)).toEqual([true, false, true]);
  });
  it("flags duplicates and handles CRLF", () => {
    const lines = parseLinks("https://a.com/1\r\nhttps://a.com/1");
    expect(lines[0].error).toBeNull();
    expect(lines[1].error).toMatch(/Duplicate/);
  });
  it("returns nothing for empty input", () => {
    expect(parseLinks("")).toEqual([]);
  });
});
