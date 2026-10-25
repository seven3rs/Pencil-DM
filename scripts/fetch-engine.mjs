#!/usr/bin/env node
// Fetches (Windows) or builds (macOS) the pinned aria2 and places it where Tauri
// expects a sidecar: src-tauri/binaries/aria2c-<target-triple>[.exe].
//
//   node scripts/fetch-engine.mjs [--target <triple>] [--verify-only] [--cache-dir <dir>]
//
// Every download is pinned to the exact URL and SHA-256 in engine-manifest.json.
// Any mismatch aborts with a non-zero exit code; there is no "skip verification"
// switch. --verify-only (used on Linux, where aria2 is not bundled) checks the
// pinned source tarball and COPYING and writes no sidecar.
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  chmodSync, copyFileSync, createReadStream, existsSync, mkdirSync, mkdtempSync,
  readFileSync, rmSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateRawSync } from "node:zlib";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const die = (msg) => { console.error(`fetch-engine: ERROR: ${msg}`); process.exit(1); };

// ---- args -------------------------------------------------------------------
const args = process.argv.slice(2);
const opt = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : undefined; };
const verifyOnly = args.includes("--verify-only");
const cacheDir = resolve(opt("--cache-dir") ?? process.env.PDM_ENGINE_CACHE ?? join(root, ".engine-cache"));

// ---- manifest ---------------------------------------------------------------
const manifest = JSON.parse(readFileSync(join(root, "engine-manifest.json"), "utf8"));
const { source, sidecars, platforms } = manifest;
if (!source || !sidecars) die("engine-manifest.json lacks `source` or `sidecars`");

function hostKey() {
  const os = { win32: "windows", darwin: "macos", linux: "linux" }[process.platform];
  const arch = { x64: "x86_64", arm64: "aarch64", ia32: "x86" }[process.arch];
  return `${os}-${arch}`;
}
function keyForTriple(triple) {
  for (const [key, e] of Object.entries(sidecars)) if (e.target_triple === triple) return key;
  return undefined;
}

// ---- helpers ----------------------------------------------------------------
function sha256File(path) {
  return new Promise((ok, fail) => {
    const h = createHash("sha256");
    createReadStream(path).on("data", (d) => h.update(d)).on("error", fail).on("end", () => ok(h.digest("hex")));
  });
}
const sha256Buf = (b) => createHash("sha256").update(b).digest("hex");

async function expectHash(path, want, what) {
  const got = await sha256File(path);
  if (got !== want.toLowerCase()) die(`${what}: SHA-256 mismatch\n  expected ${want}\n  actual   ${got}`);
  console.log(`  ok  ${what} sha256 ${got}`);
}

async function download(url, dest, wantSha, what) {
  if (!url.startsWith("https://")) die(`${what}: refusing non-HTTPS URL ${url}`);
  if (existsSync(dest) && (await sha256File(dest)) === wantSha.toLowerCase()) {
    console.log(`  cache hit: ${basename(dest)}`);
    return;
  }
  console.log(`  downloading ${url}`);
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) die(`${what}: HTTP ${res.status} for ${url}`);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, Buffer.from(await res.arrayBuffer()));
  await expectHash(dest, wantSha, what); // fatal on mismatch
}

function run(cmd, argv, options = {}) {
  const r = spawnSync(cmd, argv, { stdio: "inherit", ...options });
  if (r.error || r.status !== 0) die(`command failed: ${cmd} ${argv.join(" ")}`);
}

// Entry names are validated before anything touches the disk (zip-slip guard).
function safeEntryName(name) {
  const bad = name === "" || name.startsWith("/") || name.startsWith("\\") || name.includes("\\") ||
    name.includes(":") || /[\u0000-\u001f]/.test(name) || name.split("/").some((p) => p === ".." || p === ".");
  if (bad) die(`unsafe archive entry name: ${JSON.stringify(name)}`);
  return name;
}

// Minimal ZIP reader (central directory + deflate/stored), no dependencies.
function readZipEntry(zipPath, wantedName) {
  const buf = readFileSync(zipPath);
  let eocd = -1;
  for (let i = buf.length - 22; i >= Math.max(0, buf.length - 65557); i--) {
    if (buf.readUInt32LE(i) === 0x06054b50) { eocd = i; break; }
  }
  if (eocd < 0) die("zip: end-of-central-directory not found");
  const count = buf.readUInt16LE(eocd + 10);
  let p = buf.readUInt32LE(eocd + 16);
  let found;
  for (let n = 0; n < count; n++) {
    if (buf.readUInt32LE(p) !== 0x02014b50) die("zip: corrupt central directory");
    const method = buf.readUInt16LE(p + 10);
    const csize = buf.readUInt32LE(p + 20);
    const usize = buf.readUInt32LE(p + 24);
    const nlen = buf.readUInt16LE(p + 28), elen = buf.readUInt16LE(p + 30), clen = buf.readUInt16LE(p + 32);
    const lho = buf.readUInt32LE(p + 42);
    const name = safeEntryName(buf.toString("utf8", p + 46, p + 46 + nlen)); // validate EVERY entry
    if (name === wantedName) found = { method, csize, usize, lho };
    p += 46 + nlen + elen + clen;
  }
  if (!found) die(`zip: entry ${wantedName} not found`);
  const { method, csize, usize, lho } = found;
  if (buf.readUInt32LE(lho) !== 0x04034b50) die("zip: corrupt local header");
  const start = lho + 30 + buf.readUInt16LE(lho + 26) + buf.readUInt16LE(lho + 28);
  const raw = buf.subarray(start, start + csize);
  const out = method === 0 ? raw : method === 8 ? inflateRawSync(raw) : die(`zip: unsupported method ${method}`);
  if (out.length !== usize) die("zip: size mismatch");
  return out;
}

// Validate every tarball member path before extracting anything.
function extractTarball(tarball, destDir) {
  const list = spawnSync("tar", ["-tf", tarball], { encoding: "utf8", maxBuffer: 1 << 28 });
  if (list.status !== 0) die("tar: cannot list tarball");
  const prefix = `${source.extract_dir}/`;
  for (const line of list.stdout.split(/\r?\n/).filter(Boolean)) {
    safeEntryName(line.replace(/\/$/, ""));
    if (!line.startsWith(prefix) && line !== source.extract_dir && line !== `${source.extract_dir}/`)
      die(`tar: entry outside ${source.extract_dir}: ${line}`);
  }
  mkdirSync(destDir, { recursive: true });
  run("tar", ["-xf", tarball, "-C", destDir]);
  return join(destDir, source.extract_dir);
}

// ---- main -------------------------------------------------------------------
const work = mkdtempSync(join(tmpdir(), "pdm-engine-"));
process.on("exit", () => rmSync(work, { recursive: true, force: true }));

console.log(`aria2 ${manifest.version} (${source.tag})`);
const tarball = join(cacheDir, `aria2-${manifest.version}.tar.xz`);
console.log("source tarball:");
await download(source.url, tarball, source.sha256, "source tarball");
const srcDir = extractTarball(tarball, join(work, "src"));
const copying = join(srcDir, "COPYING");
await expectHash(copying, source.copying_sha256, "COPYING");

if (verifyOnly) { console.log("verify-only: source pins OK; no sidecar produced."); process.exit(0); }

const triple = opt("--target") ?? process.env.PDM_TARGET_TRIPLE;
const key = triple ? keyForTriple(triple) : hostKey();
if (!key || !sidecars[key]) die(`no sidecar entry for ${triple ?? hostKey()}; known: ${Object.keys(sidecars).join(", ")}`);
const sc = sidecars[key];
const exe = sc.target_triple.includes("windows") ? ".exe" : "";
const outDir = join(root, "src-tauri", "binaries");
const outBin = join(outDir, `aria2c-${sc.target_triple}${exe}`);
mkdirSync(outDir, { recursive: true });
let how;

if (sc.mode === "download") {
  const e = platforms[key];
  console.log(`prebuilt archive (${key}):`);
  const zip = join(cacheDir, basename(new URL(e.url).pathname));
  await download(e.url, zip, e.sha256, "archive");
  const member = `${e.extract_dir ? `${e.extract_dir}/` : ""}${e.bin}`;
  const data = readZipEntry(zip, member);
  if (sha256Buf(data) !== e.bin_sha256.toLowerCase())
    die(`binary: SHA-256 mismatch\n  expected ${e.bin_sha256}\n  actual   ${sha256Buf(data)}`);
  console.log(`  ok  binary sha256 ${e.bin_sha256}`);
  writeFileSync(outBin, data);
  how = `Prebuilt upstream archive ${e.url} (sha256 ${e.sha256}); ${member} extracted unmodified.`;
} else if (sc.mode === "build-from-source") {
  if (process.platform !== "darwin") die("build-from-source sidecars are built on macOS runners only");
  const arch = sc.target_triple.startsWith("aarch64") ? "arm64" : "x86_64";
  const dt = sc.deployment_target;
  const cflags = `-arch ${arch} -mmacosx-version-min=${dt} -O2`;
  console.log(`building aria2 for ${sc.target_triple} (macOS ${dt}+, system TLS)`);
  const env = { ...process.env, CC: "clang", CXX: "clang++", CFLAGS: cflags, CXXFLAGS: cflags, LDFLAGS: `-arch ${arch} -mmacosx-version-min=${dt}`,
    MACOSX_DEPLOYMENT_TARGET: dt };
  const hostTriple = process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
  const configure = ["--disable-nls", "--disable-shared", "--enable-static",
    "--with-appletls", "--without-openssl", "--without-gnutls", "--without-libgcrypt", "--without-libnettle",
    "--without-libssh2", "--without-sqlite3", "--without-libcares", "--without-libxml2", "--without-libexpat", "--without-jemalloc", "--without-tcmalloc"];
  if (sc.target_triple !== hostTriple) configure.push(`--host=${sc.target_triple}`, `--build=${hostTriple}`);
  run("./configure", configure, { cwd: srcDir, env });
  run("make", [`-j${Math.max(2, (await import("node:os")).cpus().length)}`], { cwd: srcDir, env });
  copyFileSync(join(srcDir, "src", "aria2c"), outBin);
  run("strip", ["-x", outBin]);
  const archs = spawnSync("lipo", ["-archs", outBin], { encoding: "utf8" }).stdout.trim();
  if (archs !== arch) die(`built binary has architecture "${archs}", expected "${arch}"`);
  how = `Built from ${source.url} (sha256 ${source.sha256}) with clang, target ${sc.target_triple}, macOS ${dt}+, Apple system TLS, ` +
    `no libxml2/expat (Metalink disabled), no c-ares/sqlite3/ssh2; configure flags: ${configure.join(" ")}; unpatched source.`;
} else die(`unknown sidecar mode ${sc.mode}`);

if (!exe) chmodSync(outBin, 0o755);
const binSha = await sha256File(outBin);
writeFileSync(join(outDir, "engine-sidecar.sha256"), `${binSha}\n`); // read by src-tauri/build.rs
console.log(`sidecar: ${outBin}\n  sha256 ${binSha}`);

// Compliance files bundled into the installer (git-ignored, regenerated each run).
const licDir = join(root, "src-tauri", "resources", "licenses", "aria2");
mkdirSync(licDir, { recursive: true });
copyFileSync(copying, join(licDir, "COPYING"));
writeFileSync(join(licDir, "PROVENANCE.txt"), [
  `aria2 ${manifest.version}`,
  `Upstream tag: ${source.tag}`,
  `Source tarball: ${source.url}`,
  `Source tarball SHA-256: ${source.sha256}`,
  `Bundled binary: aria2c-${sc.target_triple}${exe}`,
  `Bundled binary SHA-256: ${binSha}`,
  `How obtained: ${how}`,
  "License: GPL-2.0-or-later with an OpenSSL exception (see COPYING).",
  "Pencil Download Manager is not affiliated with the aria2 project.", "",
].join("\n"));
console.log(`licenses: ${licDir}`);
