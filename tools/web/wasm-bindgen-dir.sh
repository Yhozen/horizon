#!/usr/bin/env bash
set -euo pipefail

lockfile=${1:?Expected the Cargo.lock path}
cache=${2:?Expected a cache directory for downloaded binaries}
version=$(awk -F '"' '/^name = "wasm-bindgen"$/ { getline; print $2; exit }' "$lockfile")
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "No valid wasm-bindgen version in $lockfile" >&2; exit 1; }

matches_version() {
  local reported
  reported=$("$1" --version 2>/dev/null) || return 1
  [[ "$reported" == "wasm-bindgen $version" ]]
}

if matches_version wasm-bindgen; then
  dirname "$(command -v wasm-bindgen)"
  exit 0
fi

destination="$cache/$version"
if ! matches_version "$destination/wasm-bindgen"; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  curl -fsSL --retry 3 \
    "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$version/wasm-bindgen-$version-x86_64-unknown-linux-musl.tar.gz" \
    | tar -xz --strip-components=1 -C "$tmp" --wildcards '*/wasm-bindgen'
  matches_version "$tmp/wasm-bindgen"
  mkdir -p "$destination"
  install -m 0755 "$tmp/wasm-bindgen" "$destination/wasm-bindgen"
fi
printf '%s\n' "$destination"
