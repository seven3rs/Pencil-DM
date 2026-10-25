#!/usr/bin/env node
// Installers-only guard. Fails if any file is not an allowed installer or
// compliance file.
//
//   node scripts/check-release-assets.mjs --version 1.37.0 --dir <bundle-dir> [--platform windows|macos|linux]
//   node scripts/check-release-assets.mjs --version 1.37.0 --names-file assets.txt     (release asset names, one per line)
//   node scripts/check-release-assets.mjs --self-test
import { readdirSync, readFileSync, statSync } from "node:fs";
import { basename, join, relative } from "node:path";

const opt = (n) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : undefined; };
const version = opt("--version") ?? "0.0.0";
const compliance = (v) => [`aria2-${v}.tar.xz`, "COPYING", "ENGINE-SOURCE.txt", "SHA256SUMS"];
// Windows NSIS installers are `<name>_<ver>_<arch>-setup.exe`. `.msi` is not allowed until confirmed.
const installer = {
  windows: [/-setup\.exe$/i],
  macos: [/\.dmg$/i],
  linux: [/\.deb$/i, /\.rpm$/i],
};
// Directories Tauri uses internally; never uploaded, so ignored when scanning a bundle dir.
const intermediate = new Set(["macos"]); // bundle/macos/*.app feeds the dmg
const ignoredNames = new Set([".DS_Store"]);

function classify(name, platforms, v) {
  if (compliance(v).includes(name)) return true;
  return platforms.some((p) => installer[p].some((re) => re.test(name)));
}

export function check(names, platforms, v) {
  return names.filter((n) => !classify(basename(n), platforms, v));
}

// Scans <bundle>/<format>/<file>. Sub-directories (Tauri's unpacked deb/rpm trees,
// bundle/macos/*.app) are intermediates that tauri-action never uploads; the
// final release-asset check is the authoritative gate for what is published.
function walk(dir) {
  const out = [];
  for (const top of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, top.name);
    if (!top.isDirectory()) { if (!ignoredNames.has(top.name)) out.push(p); continue; }
    if (intermediate.has(top.name)) continue;
    for (const f of readdirSync(p, { withFileTypes: true })) if (f.isFile() && !ignoredNames.has(f.name)) out.push(join(p, f.name));
  }
  return out;
}

function report(bad, all) {
  console.log(`checked ${all.length} file(s)`);
  for (const n of all) console.log(`  ${bad.includes(n) ? "FORBIDDEN" : "allowed  "} ${n}`);
  if (bad.length) { console.error(`\nFAIL: ${bad.length} file(s) are not allowed installers or compliance files.`); process.exit(1); }
  console.log("\nOK: only allowed installers and compliance files.");
}

if (process.argv.includes("--self-test")) {
  const v = "1.37.0";
  const good = ["Pencil Download Manager_0.1.0_x64-setup.exe", "Pencil Download Manager_0.1.0_aarch64.dmg", "pdm_0.1.0_amd64.deb", "pdm-0.1.0-1.x86_64.rpm", ...compliance(v)];
  const badSet = ["pencil-download-manager.exe", "Pencil.app.tar.gz", "app.nsis.zip", "pdm.AppImage", "pdm-portable.zip", "pdm.tar.gz", "latest.json", "x.msi", "Pencil Download Manager.app", "aria2c.exe"];
  const all = ["windows", "macos", "linux"];
  let ok = check(good, all, v).length === 0;
  for (const b of badSet) if (check([b], all, v).length !== 1) { console.error(`self-test: ${b} was not rejected`); ok = false; }
  if (check(["x.dmg"], ["windows"], v).length !== 1) { console.error("self-test: .dmg accepted on windows"); ok = false; }
  console.log(ok ? "self-test passed" : "self-test FAILED"); process.exit(ok ? 0 : 1);
}

const namesFile = opt("--names-file");
if (namesFile) {
  const names = readFileSync(namesFile, "utf8").split("\n").map((s) => s.trim()).filter(Boolean);
  report(check(names, ["windows", "macos", "linux"], version), names);
} else {
  const dir = opt("--dir");
  const platform = opt("--platform") ?? { win32: "windows", darwin: "macos", linux: "linux" }[process.platform];
  if (!dir || !statSync(dir, { throwIfNoEntry: false })?.isDirectory()) { console.error("--dir must be an existing directory"); process.exit(2); }
  const files = walk(dir).map((p) => relative(dir, p));
  report(check(files, [platform], version), files);
}
