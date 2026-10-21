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

pub mod account_recovery;
pub mod alerting;
pub mod auth_2fa;
pub mod commitment_monitoring;
pub mod connection_pool;
pub mod incidents;
pub mod otel;
pub mod sdk;
pub mod session;
pub mod auth;
pub mod audit;
pub mod batch;
pub mod cache;
pub mod commitments;
pub mod licensing;
pub mod portfolio;
pub mod transfers;
pub mod valuation;
pub mod due_diligence;
pub mod marketplace_analytics;
pub mod recommendations;
pub mod collections;
pub mod analytics;
pub mod circuit_breaker;
pub mod compression;
pub mod deduplication;
pub mod distributed_tracing;
pub mod event_topics;
pub mod events;
pub mod graphql;
pub mod handlers;
pub mod ip_search;
pub mod health;
pub mod invariants;
pub mod load_balancer;
#[cfg(test)]
pub mod load_testing;
pub mod metrics;
/// #1099: activity feed, price notifications and marketplace trends.
pub mod news_feed;
pub mod middleware_pipeline;
pub mod request_queue;
pub mod request_signing;
pub mod rate_limit;
pub mod schemas;
pub mod soroban_rpc;
pub mod tracing_middleware;
pub mod versioning;
pub mod webhook;
pub mod websocket;
pub mod dependency_injection;
pub mod error_recovery;
pub mod fallback;
pub mod response;
pub mod validation;
pub mod validation_middleware;

use std::sync::{Mutex, MutexGuard};

/// Error returned when a mutex cannot be locked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    /// The mutex was poisoned by a panic in another thread.
    Poisoned,
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Poisoned => write!(f, "mutex was poisoned by a previous panic"),
        }
    }
}

impl std::error::Error for LockError {}

/// Acquire a lock, recovering the guard if the mutex was poisoned.
///
/// A poisoned mutex means a thread panicked while holding the lock. Rather than
/// propagating that panic into every later request, we log a warning and keep
/// using the (possibly inconsistent) data. See `docs/SECURITY_HARDENING.md`.
pub fn lock_or_recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("recovered poisoned mutex; continuing with possibly inconsistent data");
            poisoned.into_inner()
        }
    }
}

/// Acquire a lock, returning a structured [`LockError`] if the mutex is poisoned.
///
/// Use this when callers should surface the poisoning as an error instead of
/// silently recovering.
pub fn try_lock_or_error<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, LockError> {
    m.lock().map_err(|_| LockError::Poisoned)
}
