#!/usr/bin/env bash
set -euo pipefail

test "$(uname -m)" = x86_64
test -f Cargo.lock
prefix=/opt/horizon-ci
rust_version=1.100.0
node_version=24.22.3
node_sha256=4f2b175a6d53a5c617a80dba2676238a0688cef28c6da4eba35c7d844e66fcfb
node_archive="node-v$node_version-linux-x64.tar.xz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends clang

export PATH="$prefix/bin:$HOME/.cargo/bin:$PATH"
export RUSTUP_TOOLCHAIN="$rust_version"
rustup toolchain install "$rust_version" --no-self-update --profile minimal --target wasm32-unknown-unknown

curl -fsSL --retry 3 "https://nodejs.org/dist/v$node_version/$node_archive" -o "$tmp/$node_archive"
printf '%s  %s\n' "$node_sha256" "$tmp/$node_archive" | sha256sum --check
sudo install -d "$prefix"
sudo tar -xJf "$tmp/$node_archive" -C "$prefix" --strip-components=1 --no-same-owner
printf '%s\n' "$rust_version" | sudo tee "$prefix/rust-version" >/dev/null

pnpm_package=$(node -p 'require("./web/package.json").packageManager')
[[ "$pnpm_package" =~ ^pnpm@[0-9]+\.[0-9]+\.[0-9]+$ ]]
sudo env "PATH=$prefix/bin:/usr/bin:/bin" "$prefix/bin/npm" install --global --prefix "$prefix" \
  --ignore-scripts --no-audit --no-fund "$pnpm_package"

bindgen_dir=$(bash tools/web/wasm-bindgen-dir.sh Cargo.lock "$tmp/wasm-bindgen")
sudo install -m 0755 "$bindgen_dir/wasm-bindgen" "$prefix/bin/wasm-bindgen"

export PLAYWRIGHT_BROWSERS_PATH="$prefix/browsers"
sudo install -d -o "$(id -un)" "$PLAYWRIGHT_BROWSERS_PATH"
(
  cd web
  CI=true pnpm install --frozen-lockfile --ignore-scripts
  pnpm exec playwright install --with-deps --no-shell chromium
  chrome=$(node --input-type=module -e 'import { chromium } from "playwright"; console.log(chromium.executablePath());')
  "$chrome" --version
)

rustc --version
node --version
pnpm --version
wasm-bindgen --version
