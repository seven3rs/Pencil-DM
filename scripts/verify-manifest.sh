#!/usr/bin/env bash
# Re-downloads every archive listed in engine-manifest.json and checks both the
# archive hash and the engine binary hash. Run in CI so a changed or replaced
# upstream file is caught before a release ships.
set -euo pipefail

manifest="${1:-engine-manifest.json}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fail=0

for key in $(jq -r '.platforms | keys[]' "$manifest"); do
  url=$(jq -r ".platforms[\"$key\"].url" "$manifest")
  want_zip=$(jq -r ".platforms[\"$key\"].sha256" "$manifest")
  want_bin=$(jq -r ".platforms[\"$key\"].bin_sha256" "$manifest")
  dir=$(jq -r ".platforms[\"$key\"].extract_dir // empty" "$manifest")
  bin=$(jq -r ".platforms[\"$key\"].bin" "$manifest")

  echo "== $key"
  curl -fsSL --retry 3 -o "$work/$key.zip" "$url"
  got_zip=$(sha256sum "$work/$key.zip" | cut -d' ' -f1)
  member="${dir:+$dir/}$bin"
  got_bin=$(unzip -p "$work/$key.zip" "$member" | sha256sum | cut -d' ' -f1)

  if [ "$got_zip" != "$want_zip" ]; then echo "  archive hash mismatch: want $want_zip got $got_zip"; fail=1; else echo "  archive ok"; fi
  if [ "$got_bin" != "$want_bin" ]; then echo "  binary hash mismatch: want $want_bin got $got_bin"; fail=1; else echo "  binary ok"; fi
done

exit $fail
