#!/usr/bin/env bash
set -e
source "$HOME/.cargo/env" 2>/dev/null || true
cargo test
echo "Tests complete."
