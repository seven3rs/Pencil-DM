// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../app.css", import.meta.url), "utf8");

function tokens(block: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const m of block.matchAll(/--([a-z0-9-]+):\s*(#[0-9a-fA-F]{6})/g)) out[m[1]] = m[2];
  return out;
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const dark = tokens(css.slice(css.indexOf(":root"), css.indexOf("@media")));
const light = { ...dark, ...tokens(css.slice(css.indexOf("@media (prefers-color-scheme: light)"))) };

const textPairs: [string, string][] = [
  ["text", "bg"], ["text", "surface"], ["text", "surface-2"], ["text", "row-selected"], ["text", "row-hover"],
  ["muted", "bg"], ["muted", "surface"], ["muted", "surface-2"], ["muted", "row-selected"],
  ["accent-ink", "accent"], ["accent-text", "surface"], ["accent-text", "row-selected"],
  ["danger", "surface"], ["danger", "row-selected"], ["ok", "surface"], ["ok", "row-selected"],
];

describe.each([["dark", dark], ["light", light]])("%s theme", (_name, theme) => {
  it.each(textPairs)("%s on %s has at least 4.5:1 contrast", (fg, bg) => {
    expect(contrast(theme[fg], theme[bg])).toBeGreaterThanOrEqual(4.5);
  });
  it("keeps the pencil-yellow accent", () => {
    expect(theme.accent.toLowerCase()).toBe("#f2c230");
  });
  it("has a focus ring and input borders of at least 3:1", () => {
    expect(contrast(theme.focus, theme.surface)).toBeGreaterThanOrEqual(3);
    expect(contrast(theme["border-strong"], theme.surface)).toBeGreaterThanOrEqual(3);
  });
});
