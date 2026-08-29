#!/usr/bin/env bash
set -e
source "$HOME/.cargo/env" 2>/dev/null || true
cargo test
echo "Tests complete."

# #805: run the IP Registry CPU-instruction-budget benchmarks explicitly so a
# regression shows up even if `cargo test`'s default output is skimmed.
cargo test bench_ -p ip_registry
echo "Benchmarks complete."
