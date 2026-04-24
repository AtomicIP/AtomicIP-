# Testnet Deployment & Integration Testing Guide

This guide covers deploying Atomic Patent contracts to the Stellar testnet and
running the full integration test suite.

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | 1.70+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Stellar CLI | latest | `cargo install --locked stellar-cli` |
| jq | any | `apt install jq` / `brew install jq` |

Add the WASM target:

```bash
rustup target add wasm32-unknown-unknown
```

## 1. Configure Environment

Copy the example env file and fill in your values:

```bash
cp .env.example .env
```

Minimum required variables:

```env
STELLAR_NETWORK=testnet
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
```

## 2. Create a Deployer Identity

```bash
stellar keys generate deployer --network testnet
```

Fund it from the Stellar testnet friendbot:

```bash
stellar keys fund deployer --network testnet
```

Verify the balance:

```bash
stellar account show $(stellar keys address deployer) --network testnet
```

## 3. Deploy Contracts

```bash
./scripts/deploy_testnet.sh
```

Options:

| Flag | Effect |
|------|--------|
| `--fresh` | Regenerate deployer keys before deploying |
| `--skip-build` | Skip `cargo build` (use existing WASM) |
| `--skip-init` | Skip contract initialization |
| `--dry-run` | Print commands without executing |
| `--verbose` | Log every command |

On success the script:
1. Builds both contracts to WASM
2. Deploys `ip_registry` and captures its contract ID → `CONTRACT_IP_REGISTRY`
3. Deploys `atomic_swap` and captures its contract ID → `CONTRACT_ATOMIC_SWAP`
4. Calls `initialize` on the swap contract, wiring it to the registry
5. Validates the registry is callable
6. Writes both IDs to `.env.testnet`

## 4. Export Contract IDs

```bash
source .env.testnet
```

This sets:

```bash
export CONTRACT_IP_REGISTRY=C...
export CONTRACT_ATOMIC_SWAP=C...
```

You can also set them manually:

```bash
export CONTRACT_IP_REGISTRY=<your-ip-registry-id>
export CONTRACT_ATOMIC_SWAP=<your-atomic-swap-id>
```

## 5. Run Integration Tests

### Local (Soroban test environment — no network required)

These tests always run and cover the full swap lifecycle with real contract
calls, token balance assertions, and fee math:

```bash
cargo test --test testnet_integration
```

Tests included:

| Test | What it checks |
|------|---------------|
| `test_full_swap_flow_state_transitions` | Pending → Accepted → Completed |
| `test_token_balances_after_full_flow` | Buyer escrow + seller payout |
| `test_fee_calculation_on_completion` | 2.5% fee split to treasury |
| `test_cancel_pending_swap` | Cancel before accept, no escrow |
| `test_cancel_expired_accepted_swap_refunds_buyer` | Buyer refund after expiry |
| `test_duplicate_active_swap_rejected` | One active swap per IP |
| `test_ip_relisted_after_completion` | IP reusable after swap completes |

### Property-Based Tests (proptest)

```bash
cargo test prop_tests
```

Properties verified across random inputs:

- `prop_initiate_always_pending` — every new swap starts Pending
- `prop_completed_requires_accepted` — Completed only reachable via Accepted
- `prop_completed_swap_cannot_be_cancelled` — terminal state is immutable
- `prop_cancelled_swap_cannot_be_accepted` — cancelled is terminal
- `prop_price_preserved` — price never mutated during lifecycle
- `prop_zero_price_rejected` — price ≤ 0 always panics
- `prop_seller_buyer_recorded_correctly` — parties stored and distinct

### Live Testnet Smoke Tests

Requires `CONTRACT_IP_REGISTRY` and `CONTRACT_ATOMIC_SWAP` to be set:

```bash
source .env.testnet
cargo test --test testnet_integration -- --ignored --nocapture
```

Run a single test:

```bash
cargo test test_testnet_full_swap_flow -- --ignored --nocapture
```

## 6. Manual Verification

After deployment you can interact with the contracts directly:

```bash
# Commit an IP
stellar contract invoke \
  --id "$CONTRACT_IP_REGISTRY" \
  --source deployer \
  --network testnet \
  -- commit_ip \
  --owner $(stellar keys address deployer) \
  --commitment_hash <32-byte-hex>

# List IPs for an owner
stellar contract invoke \
  --id "$CONTRACT_IP_REGISTRY" \
  --source deployer \
  --network testnet \
  -- list_ip_by_owner \
  --owner $(stellar keys address deployer)
```

## 7. Swap Flow Reference

```
Seller                          Buyer
  │                               │
  │── initiate_swap ──────────────│  → status: Pending
  │                               │
  │                  accept_swap ─│  → status: Accepted (tokens in escrow)
  │                               │
  │── reveal_key ─────────────────│  → status: Completed
  │   (valid key)                 │    seller receives price - fee
  │                               │    treasury receives fee
```

Cancel paths:

- Seller or buyer can `cancel_swap` while Pending → status: Cancelled
- Buyer can `cancel_expired_swap` after expiry (7 days) → status: Cancelled, buyer refunded

## 8. Troubleshooting

**`IP_REGISTRY_CONTRACT_ID not set`**
Run `source .env.testnet` before running ignored tests.

**`AlreadyInitialized (#22)`**
The swap contract was already initialized. Use `--skip-init` on re-deployment or deploy fresh with `--fresh`.

**`SellerIsNotTheIPOwner (#4)`**
The `seller` address passed to `initiate_swap` must match the address used in `commit_ip`.

**`ActiveSwapAlreadyExistsForThisIpId (#5)`**
Cancel or complete the existing swap before initiating a new one for the same IP.

**Testnet friendbot rate limit**
Wait a few minutes and retry `stellar keys fund deployer --network testnet`.
