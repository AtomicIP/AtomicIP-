//! # Atomic Patent API
//!
//! A decentralized Intellectual Property registry built on Stellar Soroban smart contracts.
//!
//! ## Features
//!
//! - **IP Commitment**: Timestamp your IP with cryptographic commitments
//! - **Atomic Swaps**: Trustless patent sales with atomic swaps
//! - **WebSocket Support**: Real-time event subscriptions
//! - **Request Signing**: Secure API requests with Stellar keypair signatures
//!
//! ## API Endpoints
//!
//! ### IP Registry
//! - `POST /ip/commit` - Commit a new IP
//! - `GET /ip/{ip_id}` - Retrieve an IP record
//! - `POST /ip/transfer` - Transfer IP ownership
//! - `POST /ip/verify` - Verify a commitment
//! - `GET /ip/owner/{owner}` - List IPs by owner
//!
//! ### Atomic Swap
//! - `POST /swap/initiate` - Initiate a patent sale
//! - `POST /swap/{swap_id}/accept` - Accept a swap
//! - `POST /swap/{swap_id}/reveal` - Reveal decryption key
//! - `POST /swap/{swap_id}/cancel` - Cancel a swap
//! - `GET /swap/{swap_id}` - Get swap status
//!
//! ### WebSocket
//! - `GET /ws` - WebSocket endpoint for real-time events
//!
//! ## Authentication
//!
//! API requests can be signed using Stellar keypairs. Include the following headers:
//! - `X-Signature`: HMAC-SHA256 signature of the request
//! - `X-Timestamp`: Unix timestamp of the request
//! - `X-Public-Key`: Stellar public key

pub mod auth;
pub mod cache;
pub mod handlers;
pub mod metrics;
pub mod schemas;
pub mod webhook;
pub mod websocket;
pub mod graphql;
pub mod request_signing;
pub mod invariants;
