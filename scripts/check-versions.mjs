#!/usr/bin/env node
// Fails if the app version differs between any of the files that carry it.
//   node scripts/check-versions.mjs [--tag v0.1.0]
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");
const json = (p) => JSON.parse(read(p));
const found = [];

const cargoPackageVersion = (text) => text.match(/^\[package\][^[]*?^version\s*=\s*"([^"]+)"/ms)?.[1];
for (const p of ["crates/engine/Cargo.toml", "src-tauri/Cargo.toml"]) found.push([p, cargoPackageVersion(read(p))]);

const lock = read("Cargo.lock");
for (const name of ["pdm-engine", "pencil-download-manager"]) {
  const m = lock.match(new RegExp(`\\[\\[package\\]\\]\\nname = "${name}"\\nversion = "([^"]+)"`));
  found.push([`Cargo.lock (${name})`, m?.[1]]);
}
found.push(["src-tauri/tauri.conf.json", json("src-tauri/tauri.conf.json").version]);
found.push(["package.json", json("package.json").version]);
const pl = json("package-lock.json");
found.push(["package-lock.json", pl.version], ["package-lock.json (packages[\"\"])", pl.packages?.[""]?.version]);
for (const p of ["extension/manifest.json", "extension/manifest.firefox.json"]) found.push([p, json(p).version]);

const tagIdx = process.argv.indexOf("--tag");
if (tagIdx > 0) found.push([`git tag ${process.argv[tagIdx + 1]}`, process.argv[tagIdx + 1].replace(/^v/, "")]);

const missing = found.filter(([, v]) => !v);
const versions = new Set(found.map(([, v]) => v).filter(Boolean));
for (const [file, v] of found) console.log(`${(v ?? "MISSING").padEnd(10)} ${file}`);
if (missing.length || versions.size !== 1) {
  console.error(`\nVersion mismatch: ${[...versions].join(", ")}${missing.length ? " (and unreadable entries)" : ""}`);
  process.exit(1);
}
console.log(`\nAll ${found.length} version fields agree: ${[...versions][0]}`);
