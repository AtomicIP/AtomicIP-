# Atomic Patent — The Instant IP Ledger

[![CI](https://github.com/AtomicIP/AtomicIP-/actions/workflows/ci.yml/badge.svg)](https://github.com/AtomicIP/AtomicIP-/actions/workflows/ci.yml)
[![Security Audit](https://github.com/AtomicIP/AtomicIP-/actions/workflows/ci.yml/badge.svg)](https://github.com/AtomicIP/AtomicIP-/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/AtomicIP/AtomicIP-/branch/main/graph/badge.svg)](https://codecov.io/gh/AtomicIP/AtomicIP-)

A decentralized Intellectual Property registry built on Stellar Soroban smart contracts using Pedersen Commitments and Atomic Swaps.

In engineering, proving "Prior Art" across borders is expensive, slow, and lawyer-dependent. Atomic Patent lets you claim an idea instantly — without revealing it to competitors — and sell it globally without intermediaries.

## 🎯 What is Atomic Patent?

Atomic Patent is a Zero-Knowledge IP registry on Stellar. Engineers, inventors, and creators can:

- Commit a cryptographic hash of their design/code to the blockchain
- Prove they had the idea at a specific timestamp — without revealing the idea
- Sell the patent trustlessly via Atomic Swap — the buyer gets the decryption key in the same transaction they send payment

This Soroban implementation makes Atomic Patent:

✅ Trustless (no lawyers, no notaries, no central registry)
✅ Private (Pedersen Commitments hide your idea until you choose to reveal it)
✅ Instant (timestamp your IP in seconds, not months)
✅ Global (a mechanical engineer in Lagos can sell a design to a firm in Tokyo — no intermediary needed)

## 🚀 Features

- Claim IP: Commit a Pedersen hash of your design to Stellar with a verifiable timestamp
- Prove Prior Art: On-chain proof that you held the idea before a specific date
- Atomic Sale: Sell your patent via Atomic Swap — payment and decryption key exchange in one transaction
- Trustless Verification: If the decryption key is invalid, the payment fails automatically
- Borderless: Works for any creator, anywhere, with a Stellar wallet

## 🛠️ Quick Start

### Prerequisites

- Rust (1.70+)
- Soroban CLI
- Stellar CLI

### Build

```bash
./scripts/build.sh
```

### Test

```bash
./scripts/test.sh
```

### Deploy to Testnet

```bash
# Configure your testnet identity first
stellar keys generate deployer --network testnet

# Deploy
./scripts/deploy_testnet.sh
```

## 🌐 Testnet Deployment Status

[![Deploy to Testnet](https://github.com/AtomicIP/AtomicIP-/actions/workflows/deploy-testnet.yml/badge.svg)](https://github.com/AtomicIP/AtomicIP-/actions/workflows/deploy-testnet.yml)

Latest testnet deployment addresses are published in GitHub Actions deployment summaries. Deployments are triggered automatically on release tags (`v*`).

## 📖 Documentation

### Core Documentation
- [Architecture Overview](docs/architecture.md)
- [Commitment Scheme](docs/commitment-scheme.md)
- [Atomic Swap Flow](docs/atomic-swap.md)
- [Threat Model & Security](docs/threat-model.md)
- [Integration Guide for Wallet Providers](docs/integration-guide.md)

### Additional Resources
- [API Reference](docs/api-reference.md)
- [Security Policy](SECURITY.md)

## 📦 Release Notes and Changelog

Release notes are generated automatically from commit messages and PR metadata. Push a tag in the format `v*` (e.g., `v1.2.0`) to trigger the release workflow.

## 🎓 Smart Contract API

### IP Registry

```rust
commit_ip(owner, commitment_hash) -> u64          // Timestamp a new IP commitment
get_ip(ip_id) -> IpRecord                         // Retrieve an IP record
verify_commitment(ip_id, secret) -> bool          // Verify a commitment against a secret
list_ip_by_owner(owner) -> Vec<u64>               // List all IP IDs for an owner
batch_verify_commitments(requests) -> Vec<VerifyResult>  // #458: Verify multiple commitments with ZK proofs
assign_ip_to_category(ip_id, category_hash)       // #459: Assign IP to a hierarchical category
list_ip_by_category(owner, category_hash) -> Vec<u64>    // #459: List IPs in a category
list_owner_categories(owner) -> Vec<BytesN<32>>   // #459: List all categories for an owner

// #464: Anonymous Batch Commitments
batch_commit_ip_anonymous(blinded_owner, commitment_hashes) -> Vec<u64>  // Register commitments without revealing submitter
get_anonymous_owner(commitment_hash) -> Option<BytesN<32>>               // Retrieve blinded owner for anonymous commitment
get_blinded_owner_batch(commitment_hashes) -> Vec<Option<BytesN<32>>>   // Batch lookup of blinded owners

// #465: Batch Escrow
batch_escrow_commitments(depositor, ip_ids, release_to, timeout) -> BytesN<32>  // Escrow multiple IPs for conditional release
get_batch_escrow(escrow_id) -> Option<EscrowRecord>                              // Retrieve escrow record
release_batch_escrow(escrow_id)                                                  // Release escrowed IPs to beneficiary
cancel_batch_escrow(escrow_id)                                                   // Cancel escrow after timeout
```

### Atomic Swap

```rust
initiate_swap(ip_id, price, buyer) -> u64         // Seller initiates a patent sale
accept_swap(swap_id, payment)                     // Buyer accepts and sends payment
reveal_key(swap_id, decryption_key)               // Seller reveals key; payment releases
cancel_swap(swap_id)                              // Cancel if key is invalid or timeout

// #470: Price Oracle Integration
set_oracle(caller, oracle_address, enabled)       // Admin sets the price oracle contract
get_oracle_config() -> Option<OracleConfig>       // Query current oracle configuration
get_oracle_price(token) -> i128                   // Fetch current price from oracle
initiate_swap_with_oracle_price(...)  -> u64      // Initiate swap at oracle-determined price
```

## 🧪 Testing

Comprehensive test suite covering:

✅ IP commitment and timestamping
✅ Pedersen commitment verification
✅ Atomic swap initiation and acceptance
✅ Key reveal and payment release
✅ Invalid key rejection and payment refund
✅ Error handling and edge cases

Run tests:

```bash
cargo test
```

## 🌍 Why This Matters

Intellectual property protection today requires expensive lawyers, slow national patent offices, and jurisdiction-specific filings. This locks out independent inventors and engineers in the Global South from protecting and monetizing their ideas.

Blockchain Benefits:

- No central authority to bribe, delay, or deny
- Cryptographic proof of prior art — accepted anywhere
- Atomic Swap eliminates counterparty risk in patent sales
- Accessible to anyone with a Stellar wallet

Target Users:

- Independent engineers and inventors
- Open-source contributors protecting prior art
- Startups in emerging markets
- Any creator who can't afford a patent attorney

## 🗺️ Roadmap

- v1.0 (Current): XLM-only swaps, Pedersen commitment registry
- v1.1: USDC/EURC payment support for patent sales
- v2.0: Partial disclosure proofs (reveal claims without full design)
- v3.0: Frontend UI with wallet integration
- v4.0: Mobile app, legal document generation

## 🤝 Contributing

We welcome contributions! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [Stellar Development Foundation](https://stellar.org) for Soroban
- The global engineering community building without borders
- Drips Wave for supporting public goods funding
