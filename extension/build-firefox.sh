#!/usr/bin/env sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
OUT=${1:-"$ROOT/../dist/firefox-extension"}
mkdir -p "$OUT"
for file in background.js popup.html popup.css popup.js icon.png PRIVACY.md; do
  cp "$ROOT/$file" "$OUT/$file"
done
cp "$ROOT/manifest.firefox.json" "$OUT/manifest.json"
printf 'Firefox extension build written to %s\n' "$OUT"
