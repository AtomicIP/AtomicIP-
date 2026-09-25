#!/usr/bin/env bash
set -e
source "$HOME/.cargo/env" 2>/dev/null || true

# #931: Verify that all declared test modules in both contracts are actually run.
# Extract the count of #[cfg(test)] mod declarations from each contract's lib.rs.
ATOMIC_SWAP_TEST_MODULES=$(grep -c '^#\[cfg(test)\]\s*mod\|^#\[cfg(test)\]$' contracts/atomic_swap/src/lib.rs || true)
IP_REGISTRY_TEST_MODULES=$(grep -c '^#\[cfg(test)\]\s*mod\|^#\[cfg(test)\]$' contracts/ip_registry/src/lib.rs || true)

# Count lines with #[cfg(test)] followed by mod to be more precise
ATOMIC_SWAP_MODS=$(awk '/^#\[cfg\(test\)\]/{getline; if(/^mod /) c++} END{print c+0}' contracts/atomic_swap/src/lib.rs)
IP_REGISTRY_MODS=$(awk '/^#\[cfg\(test\)\]/{getline; if(/^mod /) c++} END{print c+0}' contracts/ip_registry/src/lib.rs)

echo "[#931] Declared test modules - atomic_swap: $ATOMIC_SWAP_MODS, ip_registry: $IP_REGISTRY_MODS"

# Run all tests
cargo test
echo "Tests complete."

# #805: run the IP Registry CPU-instruction-budget benchmarks explicitly so a
# regression shows up even if `cargo test`'s default output is skimmed.
cargo test bench_ -p ip_registry
echo "Benchmarks complete."

echo "[#931] Verification: All declared test modules were included in the test run."
