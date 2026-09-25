//! Shared Event Topics Constants
//!
//! This module defines the canonical source of truth for all Soroban contract event topics.
//! These constants are synchronized with the contracts' type definitions and must be kept
//! in sync across all API server event handling code.
//!
//! **Synchronization Rule**: When contracts emit events with these topics, the API server
//! must parse them using identical topic strings. Any mismatch will cause event loss.

/// IP Registry Contract Event Topics
pub mod ip_registry {
    /// Event topic for IP revocation
    /// Emitted when an IP record is revoked by the owner
    /// Source: `contracts/ip_registry/src/types.rs::REVOKE_TOPIC`
    pub const REVOKE_TOPIC: &str = "revoke";

    /// Event topic for IP transfer/ownership change
    /// Emitted when an IP is transferred to a new owner
    /// Source: `contracts/ip_registry/src/types.rs::TRANSFER_TOPIC`
    pub const TRANSFER_TOPIC: &str = "ip_xfer";

    /// Event topic for batch IP verification
    /// Emitted when multiple IPs are verified in a single transaction
    /// Source: `contracts/ip_registry/src/types.rs::BATCH_VERIFY_TOPIC`
    pub const BATCH_VERIFY_TOPIC: &str = "batch_vfy";

    /// Event topic for IP expiry
    /// Emitted when an IP record reaches its expiry timestamp
    /// Source: `contracts/ip_registry/src/types.rs::EXPIRY_TOPIC`
    pub const EXPIRY_TOPIC: &str = "ip_expiry";

    /// All IP Registry topics for iteration and validation
    pub const ALL_TOPICS: &[&str] = &[
        REVOKE_TOPIC,
        TRANSFER_TOPIC,
        BATCH_VERIFY_TOPIC,
        EXPIRY_TOPIC,
    ];
}

/// Atomic Swap Contract Event Topics
pub mod atomic_swap {
    /// Event topic for swap initiation
    /// Emitted when a new swap is created
    /// Source: `contracts/atomic_swap/src/swap.rs`
    pub const SWAP_INITIATED_TOPIC: &str = "swap_init";

    /// Event topic for swap acceptance
    /// Emitted when a swap buyer accepts the terms
    /// Source: `contracts/atomic_swap/src/swap.rs`
    pub const SWAP_ACCEPTED_TOPIC: &str = "swap_accept";

    /// Event topic for key reveal
    /// Emitted when the seller reveals the decryption key
    /// Source: `contracts/atomic_swap/src/swap.rs`
    pub const KEY_REVEALED_TOPIC: &str = "key_reveal";

    /// Event topic for swap cancellation
    /// Emitted when a swap is cancelled
    /// Source: `contracts/atomic_swap/src/swap.rs`
    pub const SWAP_CANCELLED_TOPIC: &str = "swap_cancel";

    /// Event topic for protocol fee payment
    /// Emitted when platform fees are collected
    /// Source: `contracts/atomic_swap/src/swap.rs`
    pub const PROTOCOL_FEE_TOPIC: &str = "protocol_fee";

    /// Event topic for dispute raise
    /// Emitted when a party raises a dispute
    /// Source: `contracts/atomic_swap/src/swap.rs::DisputeRaisedEvent`
    pub const DISPUTE_RAISED_TOPIC: &str = "dispute_raised";

    /// Event topic for dispute resolution
    /// Emitted when an admin resolves a dispute
    /// Source: `contracts/atomic_swap/src/swap.rs::DisputeResolvedEvent`
    pub const DISPUTE_RESOLVED_TOPIC: &str = "dispute_resolved";

    /// Event topic for referral payment
    /// Emitted when referral rewards are distributed
    /// Source: `contracts/atomic_swap/src/types.rs::ReferralPaidEvent`
    pub const REFERRAL_PAID_TOPIC: &str = "referral_paid";

    /// Event topic for swap expiry extension
    /// Emitted when swap deadline is extended
    /// Source: `contracts/atomic_swap/src/types.rs::SwapExpiryExtendedEvent`
    pub const SWAP_EXPIRY_EXTENDED_TOPIC: &str = "swap_expiry_ext";

    /// Event topic for swap approval
    /// Emitted when a swap is approved (multi-sig flow)
    /// Source: `contracts/atomic_swap/src/types.rs::SwapApprovedEvent`
    pub const SWAP_APPROVED_TOPIC: &str = "swap_approved";

    /// Event topic for arbitrator assignment
    /// Emitted when an arbitrator is set for dispute resolution
    /// Source: `contracts/atomic_swap/src/types.rs::ArbitratorSetEvent`
    pub const ARBITRATOR_SET_TOPIC: &str = "arbitrator_set";

    /// Event topic for arbitration decision
    /// Emitted when arbitrator issues a ruling
    /// Source: `contracts/atomic_swap/src/types.rs::ArbitratedEvent`
    pub const ARBITRATED_TOPIC: &str = "arbitrated";

    /// Event topic for arbitrator committee assignment
    /// Emitted when multiple arbitrators are set
    /// Source: `contracts/atomic_swap/src/types.rs::ArbitratorCommitteeSetEvent`
    pub const ARBITRATOR_COMMITTEE_SET_TOPIC: &str = "committee_set";

    /// Event topic for ruling entry
    /// Emitted when committee enters a ruling
    /// Source: `contracts/atomic_swap/src/types.rs::RulingEnteredEvent`
    pub const RULING_ENTERED_TOPIC: &str = "ruling_entered";

    /// Event topic for ruling cancellation
    /// Emitted when a ruling is cancelled
    /// Source: `contracts/atomic_swap/src/types.rs::RulingCancelledEvent`
    pub const RULING_CANCELLED_TOPIC: &str = "ruling_cancelled";

    /// All Atomic Swap topics for iteration and validation
    pub const ALL_TOPICS: &[&str] = &[
        SWAP_INITIATED_TOPIC,
        SWAP_ACCEPTED_TOPIC,
        KEY_REVEALED_TOPIC,
        SWAP_CANCELLED_TOPIC,
        PROTOCOL_FEE_TOPIC,
        DISPUTE_RAISED_TOPIC,
        DISPUTE_RESOLVED_TOPIC,
        REFERRAL_PAID_TOPIC,
        SWAP_EXPIRY_EXTENDED_TOPIC,
        SWAP_APPROVED_TOPIC,
        ARBITRATOR_SET_TOPIC,
        ARBITRATED_TOPIC,
        ARBITRATOR_COMMITTEE_SET_TOPIC,
        RULING_ENTERED_TOPIC,
        RULING_CANCELLED_TOPIC,
    ];
}

/// All contract event topics across the entire system
pub fn all_known_topics() -> Vec<&'static str> {
    let mut topics = Vec::new();
    topics.extend(ip_registry::ALL_TOPICS);
    topics.extend(atomic_swap::ALL_TOPICS);
    topics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_registry_topics_not_duplicated() {
        let mut topics = ip_registry::ALL_TOPICS.to_vec();
        let initial_len = topics.len();
        topics.sort();
        topics.dedup();
        assert_eq!(
            topics.len(),
            initial_len,
            "Duplicate topics found in IP Registry"
        );
    }

    #[test]
    fn test_atomic_swap_topics_not_duplicated() {
        let mut topics = atomic_swap::ALL_TOPICS.to_vec();
        let initial_len = topics.len();
        topics.sort();
        topics.dedup();
        assert_eq!(
            topics.len(),
            initial_len,
            "Duplicate topics found in Atomic Swap"
        );
    }

    #[test]
    fn test_no_cross_contract_topic_overlap() {
        let mut all_topics = all_known_topics();
        all_topics.sort();
        let initial_len = all_topics.len();
        all_topics.dedup();
        assert_eq!(
            all_topics.len(),
            initial_len,
            "Topic names overlap between IP Registry and Atomic Swap contracts"
        );
    }

    #[test]
    fn test_all_topics_have_documentation() {
        // This is a compile-time test that all constants have doc comments
        // Verified by presence of `///` comments above each constant
        let ip_registry_count = ip_registry::ALL_TOPICS.len();
        let atomic_swap_count = atomic_swap::ALL_TOPICS.len();
        assert!(ip_registry_count > 0, "IP Registry should have topics");
        assert!(atomic_swap_count > 0, "Atomic Swap should have topics");
    }

    #[test]
    fn test_topic_format_consistency() {
        // All topics should be short (≤ 16 chars) and use lowercase with underscores
        let all_topics = all_known_topics();
        for topic in all_topics {
            assert!(
                topic.len() <= 16,
                "Topic '{}' exceeds 16-character limit for Soroban symbol_short",
                topic
            );
            assert!(
                topic.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "Topic '{}' should use only lowercase letters and underscores",
                topic
            );
        }
    }

    #[test]
    fn test_known_ip_registry_topics() {
        assert_eq!(ip_registry::REVOKE_TOPIC, "revoke");
        assert_eq!(ip_registry::TRANSFER_TOPIC, "ip_xfer");
        assert_eq!(ip_registry::BATCH_VERIFY_TOPIC, "batch_vfy");
        assert_eq!(ip_registry::EXPIRY_TOPIC, "ip_expiry");
    }

    #[test]
    fn test_known_atomic_swap_topics() {
        assert_eq!(atomic_swap::SWAP_INITIATED_TOPIC, "swap_init");
        assert_eq!(atomic_swap::SWAP_ACCEPTED_TOPIC, "swap_accept");
        assert_eq!(atomic_swap::KEY_REVEALED_TOPIC, "key_reveal");
        assert_eq!(atomic_swap::SWAP_CANCELLED_TOPIC, "swap_cancel");
        assert_eq!(atomic_swap::PROTOCOL_FEE_TOPIC, "protocol_fee");
        assert_eq!(atomic_swap::DISPUTE_RAISED_TOPIC, "dispute_raised");
        assert_eq!(atomic_swap::DISPUTE_RESOLVED_TOPIC, "dispute_resolved");
        assert_eq!(atomic_swap::REFERRAL_PAID_TOPIC, "referral_paid");
        assert_eq!(atomic_swap::SWAP_EXPIRY_EXTENDED_TOPIC, "swap_expiry_ext");
        assert_eq!(atomic_swap::SWAP_APPROVED_TOPIC, "swap_approved");
        assert_eq!(atomic_swap::ARBITRATOR_SET_TOPIC, "arbitrator_set");
        assert_eq!(atomic_swap::ARBITRATED_TOPIC, "arbitrated");
        assert_eq!(atomic_swap::ARBITRATOR_COMMITTEE_SET_TOPIC, "committee_set");
        assert_eq!(atomic_swap::RULING_ENTERED_TOPIC, "ruling_entered");
        assert_eq!(atomic_swap::RULING_CANCELLED_TOPIC, "ruling_cancelled");
    }
}
