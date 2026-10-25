#!/usr/bin/env node
// Sets the app version in every file that carries it, so a release only needs:
//   node scripts/set-version.mjs 0.2.0
//   (update CHANGELOG.md, commit)
//   git tag v0.2.0 && git push origin v0.2.0
// The file list mirrors scripts/check-versions.mjs; the release workflow runs that
// script with the pushed tag and fails early if anything disagrees.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$/;

const arg = process.argv[2];
if (!arg || arg.startsWith("-")) {
  console.error("usage: node scripts/set-version.mjs <version>   (e.g. 0.2.0 or v0.2.0)");
  process.exit(2);
}
const version = arg.replace(/^v/, "");
if (!SEMVER.test(version)) {
  console.error(`"${version}" is not a valid SemVer version.`);
  process.exit(2);
}

// Each edit replaces exactly one match; zero matches means the file layout changed
// and this script (and check-versions.mjs) needs updating, so fail loudly.
const edit = (file, pattern, label = file) => {
  const path = join(root, file);
  const text = readFileSync(path, "utf8");
  let hits = 0;
  const next = text.replace(pattern, (_m, pre, _old, post = "") => {
    hits += 1;
    return `${pre}${version}${post}`;
  });
  if (hits !== 1) {
    console.error(`${label}: expected exactly one version field, found ${hits}.`);
    process.exit(1);
  }
  writeFileSync(path, next);
  console.log(`updated ${label}`);
};

// First `version = "..."` inside the [package] table.
const cargoPackage = /^(\[package\][^[]*?^version\s*=\s*")([^"]+)(")/ms;
edit("crates/engine/Cargo.toml", cargoPackage);
edit("src-tauri/Cargo.toml", cargoPackage);

for (const name of ["pdm-engine", "pencil-download-manager"]) {
  edit("Cargo.lock", new RegExp(`(\\[\\[package\\]\\]\\nname = "${name}"\\nversion = ")([^"]+)(")`), `Cargo.lock (${name})`);
}

// Top-level "version" (two-space indent) in the JSON files.
const jsonTop = /^(  "version":\s*")([^"]+)(")/m;
for (const f of ["package.json", "src-tauri/tauri.conf.json", "extension/manifest.json", "extension/manifest.firefox.json"]) {
  edit(f, jsonTop);
}
edit("package-lock.json", jsonTop, "package-lock.json");
edit("package-lock.json", /^(  "packages":\s*\{\s*"":\s*\{[^}]*?"version":\s*")([^"]+)(")/m, 'package-lock.json (packages[""])');

console.log(`\nVersion set to ${version}. Add a "## ${version}" section to CHANGELOG.md, then commit and tag v${version}.`);
