# Developer Onboarding Guide

Welcome to the Atomic Patent project! This guide will help you get started with the codebase, set up your development environment, and understand our contribution workflow.

## 🚀 Getting Started

### 1. Prerequisites

Ensure you have the following tools installed:

- **Rust (1.70+):** `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **WASM Target:** `rustup target add wasm32-unknown-unknown`
- **Stellar CLI:** `cargo install stellar-cli --locked`
- **jq:** `sudo apt install jq` (for deployment scripts)

### 2. Fork and Clone

1. Fork the repository on GitHub.
2. Clone your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/AtomicIP-.git
   cd AtomicIP-
   ```

### 3. Environment Setup

Copy the example environment file:
```bash
cp .env.example .env
```
(No need to change anything yet for local development).

## 🛠️ Development Workflow

### Building Contracts

We use a helper script to build all contracts:
```bash
./scripts/build.sh
```
This produces WASM files in `target/wasm32-unknown-unknown/release/`.

### Running Tests

Run the full test suite (unit and integration tests):
```bash
./scripts/test.sh
# Or directly via cargo
cargo test
```

### Coding Standards

- **Formatting:** We use `cargo fmt`. Please run it before committing.
- **Linting:** We use `cargo clippy`. Ensure your code is clippy-clean.
- **Documentation:** Use doc comments (`///`) for all public functions and structs.

## 🚢 Deployment

### Testnet Deployment

To deploy your own instance to the Stellar testnet:

1. **Generate a deployer key:**
   ```bash
   stellar keys generate deployer --network testnet
   ```

2. **Run the deployment script:**
   ```bash
   ./scripts/deploy.sh --network testnet
   ```
   This will build, deploy, and initialize the contracts, then save the contract IDs to `.env.testnet`.

## 📂 Repository Structure

- `contracts/`: Soroban smart contract source code.
  - `ip-registry/`: The core IP ledger.
  - `atomic-swap/`: Logic for trustless patent sales.
- `api-server/`: Rust-based REST API for interacting with the contracts.
- `client/`: React-based frontend application.
- `docs/`: Technical documentation and guides.
- `scripts/`: Helper scripts for build, test, and deploy.

## 🤝 How to Contribute

1. **Find an issue:** Check the [GitHub Issues](https://github.com/AtomicIP/AtomicIP-/issues) for `wave-ready` or `good first issue` labels.
2. **Create a branch:** `git checkout -b feat/your-feature-name`.
3. **Commit changes:** `git commit -m "feat: add amazing feature"`. Follow [Conventional Commits](https://www.conventionalcommits.org/).
4. **Push and PR:** Push to your fork and open a Pull Request to the `main` branch of the upstream repository.

## 📚 Further Reading

- [Architecture Overview](architecture.md)
- [API Reference](api-reference.md)
- [Stellar Soroban Docs](https://soroban.stellar.org/docs)

---

Happy coding! 🌊
