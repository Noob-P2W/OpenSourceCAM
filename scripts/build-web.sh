#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown
if ! command -v wasm-bindgen >/dev/null || ! wasm-bindgen --version | grep -q '0.2.126'; then
  cargo install wasm-bindgen-cli --version 0.2.126 --locked
fi
cargo build --locked --lib --no-default-features --features web --target wasm32-unknown-unknown --release
mkdir -p web/pkg
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/opensourcecam.wasm
