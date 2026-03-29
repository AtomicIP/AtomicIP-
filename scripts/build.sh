#!/usr/bin/env bash
set -e
source "$HOME/.cargo/env" 2>/dev/null || true
cargo build --target wasm32-unknown-unknown --release
echo "Build complete."
