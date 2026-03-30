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

### Setup Environment

Copy the example environment file:

```bash
cp .env.example .env
```

Configure your environment variables in `.env`:

```env
# Network configuration
STELLAR_NETWORK=testnet
STELLAR_RPC_URL=https://soroban-testnet.stellar.org

# Contract addresses (after deployment)
CONTRACT_IP_REGISTRY=<your-contract-id>
CONTRACT_ATOMIC_SWAP=<your-contract-id>

# Frontend configuration
VITE_STELLAR_NETWORK=testnet
VITE_STELLAR_RPC_URL=https://soroban-testnet.stellar.org
```

Network configurations are defined in `environments.toml`:

- `testnet` — Stellar testnet
- `mainnet` — Stellar mainnet
- `futurenet` — Stellar futurenet
- `standalone` — Local development

### Deploy to Testnet

```bash
# Configure your testnet identity first
stellar keys generate deployer --network testnet

# Deploy
./scripts/deploy_testnet.sh
```

### Run Demo

Follow the step-by-step guide in `demo/demo-script.md`

## 🌐 Testnet Deployment Status

[![Deploy to Testnet](https://github.com/AtomicIP/AtomicIP-/actions/workflows/deploy-testnet.yml/badge.svg)](https://github.com/AtomicIP/AtomicIP-/actions/workflows/deploy-testnet.yml)

Latest testnet deployment addresses are published in GitHub Actions deployment summaries. Deployments are triggered automatically on release tags (`v*`).

## 📖 Documentation

- [Architecture Overview](docs/architecture.md)
- [Commitment Scheme](docs/commitment-scheme.md)
- [Changelog Format](docs/changelog-format.md)
- [Atomic Swap Flow](docs/atomic-swap.md)
- [Threat Model & Security](docs/threat-model.md)
- [Integration Guide for Wallet Providers](docs/integration-guide.md)
- [Security Policy](SECURITY.md)
- [Roadmap](docs/roadmap.md)

## 📦 Release Notes and Changelog

- Release notes are generated automatically from commit messages and PR metadata.
- Push a tag in the format `v*` (for example, `v1.2.0`) to trigger the release workflow.
- The release workflow uses `git-cliff` with configuration in `cliff.toml`.

## 🎓 Smart Contract API

### IP Registry

```rust
commit_ip(owner, commitment_hash) -> u64          // Timestamp a new IP commitment
get_ip(ip_id) -> IpRecord                         // Retrieve an IP record
verify_commitment(ip_id, secret) -> bool          // Verify a commitment against a secret
list_ip_by_owner(owner) -> Vec<u64>               // List all IP IDs for an owner
```

### Atomic Swap

```rust
initiate_swap(ip_id, price, buyer) -> u64         // Seller initiates a patent sale
accept_swap(swap_id, payment)                     // Buyer accepts and sends payment
reveal_key(swap_id, decryption_key)               // Seller reveals key; payment releases
cancel_swap(swap_id)                              // Cancel if key is invalid or timeout
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

See [docs/roadmap.md](docs/roadmap.md) for details.

## 🤝 Contributing

We welcome contributions! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

See our [Code of Conduct](CODE_OF_CONDUCT.md) and [Contributing Guidelines](CONTRIBUTING.md).

## 🌊 Drips Wave Contributors

This project participates in Drips Wave — a contributor funding program! Check out:

- [Wave Contributor Guide](docs/wave-guide.md) — How to earn funding for contributions
- [Wave-Ready Issues](https://github.com/issues?q=label%3Awave-ready) — Funded issues ready to tackle
- GitHub Issues labeled with `wave-ready` — Earn 100–200 points per issue

Issues are categorized as:

- `trivial` (100 points) — Documentation, simple tests, minor fixes
- `medium` (150 points) — Helper functions, validation logic, moderate features
- `high` (200 points) — Core features, complex integrations, security enhancements

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [Stellar Development Foundation](https://stellar.org) for Soroban
- The global engineering community building without borders
- Drips Wave for supporting public goods funding
