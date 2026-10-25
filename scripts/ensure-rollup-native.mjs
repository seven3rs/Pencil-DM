#!/usr/bin/env node
// Work around npm/cli#4828: package-lock.json only pins Linux's optional
// @rollup/rollup-* binary, so on Windows/macOS even `npm install` leaves the
// native package out and `vite build` dies with "Cannot find module".
// Installs the matching native package at the exact version rollup declares,
// without touching package.json or package-lock.json. No-op when present.
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';

const require = createRequire(import.meta.url);
const rollupPkg = JSON.parse(readFileSync(require.resolve('rollup/package.json'), 'utf8'));
const optional = rollupPkg.optionalDependencies ?? {};
const prefix = `@rollup/rollup-${process.platform}-${process.arch}`;
let names = Object.keys(optional).filter((n) => n === prefix || n.startsWith(`${prefix}-`));
if (process.platform === 'win32') names = names.filter((n) => n.endsWith('-msvc'));
if (process.platform === 'linux') names = names.filter((n) => n.endsWith('-gnu'));
if (names.length !== 1) {
  console.error(`ensure-rollup-native: expected 1 candidate for ${prefix}, got [${names}]`);
  process.exit(1);
}
const [name] = names;
try {
  require.resolve(name);
  console.log(`ensure-rollup-native: ${name} already installed`);
  process.exit(0);
} catch {}
const spec = `${name}@${optional[name]}`;
if (!/^@rollup\/rollup-[a-z0-9-]+@\d+\.\d+\.\d+$/.test(spec)) {
  console.error(`ensure-rollup-native: refusing odd spec ${spec}`);
  process.exit(1);
}
console.log(`ensure-rollup-native: installing ${spec}`);
execFileSync('npm', ['install', '--no-save', '--no-audit', '--no-fund', spec], {
  stdio: 'inherit',
  shell: process.platform === 'win32',
});
require.resolve(name);
console.log('ensure-rollup-native: ok');
