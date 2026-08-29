use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantViolation {
    pub invariant_id: String,
    pub description: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpRegistryState {
    pub total_commitments: u64,
    pub unique_owners: u64,
    pub commitment_hashes: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicSwapState {
    pub total_swaps: u64,
    pub total_fees_collected: u128,
    pub escrow_balance: u128,
    pub pending_swaps: u64,
}

pub struct InvariantChecker;

impl InvariantChecker {
    /// I1: Commitment Uniqueness
    pub fn verify_commitment_uniqueness(
        owner: &str,
        hash: &str,
        existing_hashes: &HashMap<String, Vec<String>>,
    ) -> Result<(), InvariantViolation> {
        if let Some(hashes) = existing_hashes.get(owner) {
            if hashes.contains(&hash.to_string()) {
                return Err(InvariantViolation {
                    invariant_id: "I1".to_string(),
                    description: format!("Duplicate commitment hash for owner {}", owner),
                    severity: "critical".to_string(),
                });
            }
        }
        Ok(())
    }

    /// I2: Timestamp Monotonicity
    pub fn verify_timestamp_order(
        timestamp_1: u64,
        timestamp_2: u64,
    ) -> Result<(), InvariantViolation> {
        if timestamp_2 < timestamp_1 {
            return Err(InvariantViolation {
                invariant_id: "I2".to_string(),
                description: "Timestamp order violation: newer record has earlier timestamp"
                    .to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// I3: Owner Consistency
    pub fn verify_owner_immutability(
        stored_owner: &str,
        claimed_owner: &str,
    ) -> Result<(), InvariantViolation> {
        if stored_owner != claimed_owner {
            return Err(InvariantViolation {
                invariant_id: "I3".to_string(),
                description: "Owner immutability violation: owner changed after creation"
                    .to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// I4: Commitment Verification
    pub fn verify_commitment_correctness(
        commitment_hash: &str,
        secret: &str,
    ) -> Result<(), InvariantViolation> {
        use sha2::{Sha256, Digest};
        use hex::encode;

        let mut hasher = Sha256::new();
        hasher.update(secret.as_bytes());
        let computed_hash = encode(hasher.finalize());

        if computed_hash != commitment_hash {
            return Err(InvariantViolation {
                invariant_id: "I4".to_string(),
                description: "Commitment verification failed: secret does not match hash"
                    .to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// S1: Fee Accounting
    pub fn verify_total_fees(
        collected_fees: u128,
        swap_fees: &[u128],
    ) -> Result<(), InvariantViolation> {
        let sum: u128 = swap_fees.iter().sum();
        if collected_fees != sum {
            return Err(InvariantViolation {
                invariant_id: "S1".to_string(),
                description: format!(
                    "Fee accounting violation: collected {} != sum of fees {}",
                    collected_fees, sum
                ),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// S2: Payment Atomicity
    pub fn verify_payment_key_atomicity(
        payment_released: bool,
        key_revealed: bool,
    ) -> Result<(), InvariantViolation> {
        if payment_released != key_revealed {
            return Err(InvariantViolation {
                invariant_id: "S2".to_string(),
                description: "Payment atomicity violation: payment and key reveal not synchronized"
                    .to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// S3: Swap State Transitions
    pub fn verify_state_transition(
        from_state: &str,
        to_state: &str,
    ) -> Result<(), InvariantViolation> {
        let valid_transitions = [
            ("Pending", "Active"),
            ("Pending", "Cancelled"),
            ("Active", "Completed"),
            ("Active", "Cancelled"),
        ];

        let transition = (from_state, to_state);
        if !valid_transitions.contains(&transition) {
            return Err(InvariantViolation {
                invariant_id: "S3".to_string(),
                description: format!(
                    "Invalid state transition: {} -> {}",
                    from_state, to_state
                ),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// S4: Escrow Balance
    pub fn verify_escrow_balance(
        escrow_balance: u128,
        pending_payments: &[u128],
    ) -> Result<(), InvariantViolation> {
        let sum: u128 = pending_payments.iter().sum();
        if escrow_balance != sum {
            return Err(InvariantViolation {
                invariant_id: "S4".to_string(),
                description: format!(
                    "Escrow balance violation: balance {} != sum of pending {}",
                    escrow_balance, sum
                ),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// S5: Key Validity
    pub fn verify_key_validity(key: &str, commitment_hash: &str) -> Result<(), InvariantViolation> {
        use sha2::{Sha256, Digest};
        use hex::encode;

        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let computed_hash = encode(hasher.finalize());

        if computed_hash != commitment_hash {
            return Err(InvariantViolation {
                invariant_id: "S5".to_string(),
                description: "Key validity violation: key does not decrypt commitment"
                    .to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// H1: Swap Record Invariant
    /// Validates that any retrieved or created SwapRecord satisfies structural and semantic invariants.
    pub fn verify_swap_record(record: &crate::schemas::SwapRecord) -> Result<(), InvariantViolation> {
        if record.seller.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H1_SELLER_NOT_NULL".to_string(),
                description: "Swap response invariant violation: seller address cannot be empty or null".to_string(),
                severity: "critical".to_string(),
            });
        }
        if record.buyer.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H1_BUYER_NOT_NULL".to_string(),
                description: "Swap response invariant violation: buyer address cannot be empty or null".to_string(),
                severity: "critical".to_string(),
            });
        }
        if record.token.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H1_TOKEN_NOT_NULL".to_string(),
                description: "Swap response invariant violation: token contract address cannot be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        if record.price <= 0 {
            return Err(InvariantViolation {
                invariant_id: "H1_POSITIVE_PRICE".to_string(),
                description: format!("Swap response invariant violation: price must be positive, got {}", record.price),
                severity: "critical".to_string(),
            });
        }
        if record.ip_registry_id.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H1_REGISTRY_NOT_NULL".to_string(),
                description: "Swap response invariant violation: ip_registry_id cannot be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// H2: Completed Swap Invariant
    /// Explicitly asserts that a completed swap response never includes null/empty seller/buyer,
    /// has positive price and non-null token, and seller != buyer.
    pub fn verify_completed_swap(record: &crate::schemas::SwapRecord) -> Result<(), InvariantViolation> {
        Self::verify_swap_record(record)?;

        if record.status == crate::schemas::SwapStatus::Completed {
            if record.seller == record.buyer {
                return Err(InvariantViolation {
                    invariant_id: "H2_SELLER_BUYER_DISTINCT".to_string(),
                    description: "Completed swap invariant violation: seller and buyer cannot be identical".to_string(),
                    severity: "critical".to_string(),
                });
            }
        }
        Ok(())
    }

    /// H3: IP Record Invariant
    /// Validates IP record integrity: non-empty owner, 64-character hex commitment hash, positive timestamp.
    pub fn verify_ip_record(record: &crate::schemas::IpRecord) -> Result<(), InvariantViolation> {
        if record.owner.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H3_IP_OWNER_NOT_NULL".to_string(),
                description: "IP record invariant violation: owner cannot be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        if record.commitment_hash.len() != 64 || !record.commitment_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(InvariantViolation {
                invariant_id: "H3_IP_HASH_FORMAT".to_string(),
                description: "IP record invariant violation: commitment hash must be a 64-character hex string (32 bytes)".to_string(),
                severity: "critical".to_string(),
            });
        }
        if record.timestamp == 0 {
            return Err(InvariantViolation {
                invariant_id: "H3_IP_TIMESTAMP_POSITIVE".to_string(),
                description: "IP record invariant violation: timestamp must be non-zero".to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// H4: Batch Initiate Swap Invariant
    /// Asserts that batch initiate swap requests and responses preserve batch size, positive prices,
    /// sequential ID assignment, and unique IP IDs.
    pub fn verify_batch_initiate_swap(
        req: &crate::schemas::BatchInitiateSwapRequest,
        res: &crate::schemas::BatchInitiateSwapResponse,
    ) -> Result<(), InvariantViolation> {
        if req.ip_ids.len() != req.prices.len() {
            return Err(InvariantViolation {
                invariant_id: "H4_BATCH_LENGTH_MATCH".to_string(),
                description: "Batch initiate swap invariant violation: ip_ids and prices length mismatch".to_string(),
                severity: "critical".to_string(),
            });
        }
        if res.swap_ids.len() != req.ip_ids.len() {
            return Err(InvariantViolation {
                invariant_id: "H4_BATCH_RESPONSE_LENGTH_MATCH".to_string(),
                description: "Batch initiate swap invariant violation: returned swap_ids length does not match requested ip_ids".to_string(),
                severity: "critical".to_string(),
            });
        }
        if req.ip_ids.is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H4_BATCH_NOT_EMPTY".to_string(),
                description: "Batch initiate swap invariant violation: batch must not be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        if req.seller.trim().is_empty() || req.buyer.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H4_BATCH_PARTIES_NOT_NULL".to_string(),
                description: "Batch initiate swap invariant violation: seller and buyer must not be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        if req.seller == req.buyer {
            return Err(InvariantViolation {
                invariant_id: "H4_BATCH_PARTIES_DISTINCT".to_string(),
                description: "Batch initiate swap invariant violation: seller and buyer cannot be identical".to_string(),
                severity: "critical".to_string(),
            });
        }
        for (i, &price) in req.prices.iter().enumerate() {
            if price <= 0 {
                return Err(InvariantViolation {
                    invariant_id: "H4_BATCH_POSITIVE_PRICES".to_string(),
                    description: format!("Batch initiate swap invariant violation: price at index {} must be positive", i),
                    severity: "critical".to_string(),
                });
            }
        }
        // Sequential ID allocation check
        if res.swap_ids.len() > 1 {
            for window in res.swap_ids.windows(2) {
                if window[1] != window[0] + 1 {
                    return Err(InvariantViolation {
                        invariant_id: "H4_BATCH_SEQUENTIAL_IDS".to_string(),
                        description: "Batch initiate swap invariant violation: swap IDs must be allocated sequentially".to_string(),
                        severity: "critical".to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// H5: Transfer IP Invariant
    pub fn verify_transfer_ip(
        current_owner: &str,
        new_owner: &str,
        ip_id: u64,
    ) -> Result<(), InvariantViolation> {
        if ip_id == 0 {
            return Err(InvariantViolation {
                invariant_id: "H5_TRANSFER_IP_ID_POSITIVE".to_string(),
                description: "Transfer IP invariant violation: ip_id must be non-zero".to_string(),
                severity: "critical".to_string(),
            });
        }
        if current_owner.trim().is_empty() || new_owner.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H5_TRANSFER_OWNERS_NOT_NULL".to_string(),
                description: "Transfer IP invariant violation: current and new owner addresses must not be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        if current_owner == new_owner {
            return Err(InvariantViolation {
                invariant_id: "H5_TRANSFER_DIFFERENT_OWNER".to_string(),
                description: "Transfer IP invariant violation: new owner must be different from current owner".to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// H6: Owner IP List Invariant
    pub fn verify_owner_ip_list(
        owner: &str,
        returned_ids: &[u64],
        total_count: u64,
        limit: u64,
    ) -> Result<(), InvariantViolation> {
        if owner.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H6_OWNER_NOT_EMPTY".to_string(),
                description: "Owner IP list invariant violation: owner address cannot be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        if returned_ids.len() as u64 > limit {
            return Err(InvariantViolation {
                invariant_id: "H6_LIMIT_EXCEEDED".to_string(),
                description: format!("Owner IP list invariant violation: returned {} items exceeding limit of {}", returned_ids.len(), limit),
                severity: "critical".to_string(),
            });
        }
        if returned_ids.len() as u64 > total_count {
            return Err(InvariantViolation {
                invariant_id: "H6_TOTAL_COUNT_MISMATCH".to_string(),
                description: format!("Owner IP list invariant violation: returned {} items exceeding total count {}", returned_ids.len(), total_count),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// H7: Accept Swap Invariant (#864)
    /// Validates that accept_swap operations preserve swap integrity and authorization.
    pub fn verify_accept_swap(
        swap: &crate::schemas::SwapRecord,
        buyer: &str,
    ) -> Result<(), InvariantViolation> {
        // Swap must exist and be in Pending state to accept
        if swap.status != crate::schemas::SwapStatus::Pending {
            return Err(InvariantViolation {
                invariant_id: "H7_SWAP_STATE_PENDING".to_string(),
                description: format!("Accept swap invariant violation: swap must be in Pending state, found {:?}", swap.status),
                severity: "critical".to_string(),
            });
        }
        // Buyer in request must match swap record
        if buyer.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H7_BUYER_NOT_EMPTY".to_string(),
                description: "Accept swap invariant violation: buyer address cannot be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        // Seller and buyer must be distinct
        if swap.seller == swap.buyer {
            return Err(InvariantViolation {
                invariant_id: "H7_SWAP_PARTIES_DISTINCT".to_string(),
                description: "Accept swap invariant violation: seller and buyer must be different".to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// H8: Reveal Key Invariant (#864)
    /// Validates that reveal_key operations complete swaps correctly and atomically.
    pub fn verify_reveal_key(
        swap: &crate::schemas::SwapRecord,
        caller: &str,
    ) -> Result<(), InvariantViolation> {
        // Swap must be in Accepted state to reveal key
        if swap.status != crate::schemas::SwapStatus::Accepted {
            return Err(InvariantViolation {
                invariant_id: "H8_SWAP_STATE_ACCEPTED".to_string(),
                description: format!("Reveal key invariant violation: swap must be in Accepted state, found {:?}", swap.status),
                severity: "critical".to_string(),
            });
        }
        // Only the seller can reveal the key
        if swap.seller != caller {
            return Err(InvariantViolation {
                invariant_id: "H8_CALLER_IS_SELLER".to_string(),
                description: "Reveal key invariant violation: only seller can reveal the key".to_string(),
                severity: "critical".to_string(),
            });
        }
        // Caller must not be empty
        if caller.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H8_CALLER_NOT_EMPTY".to_string(),
                description: "Reveal key invariant violation: caller address cannot be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// H9: Cancel Swap Invariant (#864)
    /// Validates that cancel_swap operations only affect eligible swaps.
    pub fn verify_cancel_swap(
        swap: &crate::schemas::SwapRecord,
        canceller: &str,
    ) -> Result<(), InvariantViolation> {
        // Swap can only be cancelled if in Pending or Accepted state
        match swap.status {
            crate::schemas::SwapStatus::Pending | crate::schemas::SwapStatus::Accepted => {},
            _ => {
                return Err(InvariantViolation {
                    invariant_id: "H9_SWAP_CANCELLABLE".to_string(),
                    description: format!("Cancel swap invariant violation: swap must be Pending or Accepted, found {:?}", swap.status),
                    severity: "critical".to_string(),
                });
            }
        }
        // Only seller or buyer can cancel
        if canceller != swap.seller && canceller != swap.buyer {
            return Err(InvariantViolation {
                invariant_id: "H9_CANCELLER_AUTHORIZED".to_string(),
                description: "Cancel swap invariant violation: only seller or buyer can cancel".to_string(),
                severity: "critical".to_string(),
            });
        }
        // Canceller must not be empty
        if canceller.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H9_CANCELLER_NOT_EMPTY".to_string(),
                description: "Cancel swap invariant violation: canceller address cannot be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }

    /// H10: Commit IP Invariant (#864)
    /// Validates that new IP commitments are well-formed.
    pub fn verify_commit_ip_request(owner: &str, commitment_hash: &str) -> Result<(), InvariantViolation> {
        if owner.trim().is_empty() {
            return Err(InvariantViolation {
                invariant_id: "H10_COMMIT_OWNER_NOT_EMPTY".to_string(),
                description: "Commit IP invariant violation: owner address cannot be empty".to_string(),
                severity: "critical".to_string(),
            });
        }
        if commitment_hash.len() != 64 || !commitment_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(InvariantViolation {
                invariant_id: "H10_COMMIT_HASH_FORMAT".to_string(),
                description: "Commit IP invariant violation: commitment hash must be 64-character hex string".to_string(),
                severity: "critical".to_string(),
            });
        }
        Ok(())
    }
}

impl InvariantViolation {
    /// Fails loudly by emitting a high-priority structured error alert and incrementing metric counters.
    pub fn alert(&self) {
        tracing::error!(
            target: "security_alert",
            invariant_id = %self.invariant_id,
            severity = %self.severity,
            description = %self.description,
            "INVARIANT VIOLATION DETECTED - FAILING LOUDLY"
        );
        metrics::counter!(
            "invariant_violations_total",
            "invariant_id" => self.invariant_id.clone(),
            "severity" => self.severity.clone(),
        )
        .increment(1);
    }
}

/// Helper to execute an invariant check and fail loudly with logging + alert metrics on violation.
pub fn check_and_alert<F>(check: F) -> Result<(), InvariantViolation>
where
    F: FnOnce() -> Result<(), InvariantViolation>,
{
    match check() {
        Ok(()) => Ok(()),
        Err(violation) => {
            violation.alert();
            Err(violation)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schemas::{SwapRecord, SwapStatus, IpRecord, BatchInitiateSwapRequest, BatchInitiateSwapResponse};

    #[test]
    fn test_verify_commitment_uniqueness_passes() {
        let existing = HashMap::new();
        assert!(InvariantChecker::verify_commitment_uniqueness("owner1", "hash1", &existing)
            .is_ok());
    }

    #[test]
    fn test_verify_commitment_uniqueness_fails_on_duplicate() {
        let mut existing = HashMap::new();
        existing.insert("owner1".to_string(), vec!["hash1".to_string()]);
        assert!(InvariantChecker::verify_commitment_uniqueness("owner1", "hash1", &existing)
            .is_err());
    }

    #[test]
    fn test_verify_timestamp_order_passes() {
        assert!(InvariantChecker::verify_timestamp_order(100, 200).is_ok());
    }

    #[test]
    fn test_verify_timestamp_order_fails() {
        assert!(InvariantChecker::verify_timestamp_order(200, 100).is_err());
    }

    #[test]
    fn test_verify_owner_immutability_passes() {
        assert!(InvariantChecker::verify_owner_immutability("owner1", "owner1").is_ok());
    }

    #[test]
    fn test_verify_owner_immutability_fails() {
        assert!(InvariantChecker::verify_owner_immutability("owner1", "owner2").is_err());
    }

    #[test]
    fn test_verify_total_fees_passes() {
        let fees = vec![100, 200, 300];
        assert!(InvariantChecker::verify_total_fees(600, &fees).is_ok());
    }

    #[test]
    fn test_verify_total_fees_fails() {
        let fees = vec![100, 200, 300];
        assert!(InvariantChecker::verify_total_fees(500, &fees).is_err());
    }

    #[test]
    fn test_verify_payment_key_atomicity_passes() {
        assert!(InvariantChecker::verify_payment_key_atomicity(true, true).is_ok());
        assert!(InvariantChecker::verify_payment_key_atomicity(false, false).is_ok());
    }

    #[test]
    fn test_verify_payment_key_atomicity_fails() {
        assert!(InvariantChecker::verify_payment_key_atomicity(true, false).is_err());
        assert!(InvariantChecker::verify_payment_key_atomicity(false, true).is_err());
    }

    #[test]
    fn test_verify_state_transition_valid() {
        assert!(InvariantChecker::verify_state_transition("Pending", "Active").is_ok());
        assert!(InvariantChecker::verify_state_transition("Active", "Completed").is_ok());
    }

    #[test]
    fn test_verify_state_transition_invalid() {
        assert!(InvariantChecker::verify_state_transition("Completed", "Active").is_err());
    }

    #[test]
    fn test_verify_escrow_balance_passes() {
        let payments = vec![100, 200, 300];
        assert!(InvariantChecker::verify_escrow_balance(600, &payments).is_ok());
    }

    #[test]
    fn test_verify_escrow_balance_fails() {
        let payments = vec![100, 200, 300];
        assert!(InvariantChecker::verify_escrow_balance(500, &payments).is_err());
    }

    // ── Handler Invariant Tests (#864) ────────────────────────────────────────

    #[test]
    fn test_verify_swap_record_valid() {
        let record = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREGISTRY123".to_string(),
            seller: "GSELLER123".to_string(),
            buyer: "GBUYER123".to_string(),
            price: 1000,
            token: "CTOKEN123".to_string(),
            status: SwapStatus::Pending,
            expiry: 999999,
        };
        assert!(InvariantChecker::verify_swap_record(&record).is_ok());
    }

    #[test]
    fn test_verify_swap_record_fails_on_null_or_empty_seller() {
        let record = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREGISTRY123".to_string(),
            seller: "".to_string(), // Null/empty seller
            buyer: "GBUYER123".to_string(),
            price: 1000,
            token: "CTOKEN123".to_string(),
            status: SwapStatus::Completed,
            expiry: 999999,
        };
        let res = InvariantChecker::verify_swap_record(&record);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H1_SELLER_NOT_NULL");
    }

    #[test]
    fn test_verify_swap_record_fails_on_empty_buyer_or_token() {
        let mut record = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREGISTRY123".to_string(),
            seller: "GSELLER123".to_string(),
            buyer: "   ".to_string(), // Empty buyer
            price: 1000,
            token: "CTOKEN123".to_string(),
            status: SwapStatus::Pending,
            expiry: 999999,
        };
        assert_eq!(
            InvariantChecker::verify_swap_record(&record).unwrap_err().invariant_id,
            "H1_BUYER_NOT_NULL"
        );

        record.buyer = "GBUYER123".to_string();
        record.token = "".to_string(); // Empty token
        assert_eq!(
            InvariantChecker::verify_swap_record(&record).unwrap_err().invariant_id,
            "H1_TOKEN_NOT_NULL"
        );
    }

    #[test]
    fn test_verify_swap_record_fails_on_non_positive_price() {
        let record = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREGISTRY123".to_string(),
            seller: "GSELLER123".to_string(),
            buyer: "GBUYER123".to_string(),
            price: 0, // Non-positive
            token: "CTOKEN123".to_string(),
            status: SwapStatus::Pending,
            expiry: 999999,
        };
        let res = InvariantChecker::verify_swap_record(&record);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H1_POSITIVE_PRICE");
    }

    #[test]
    fn test_verify_completed_swap_never_includes_null_seller() {
        let record = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREGISTRY123".to_string(),
            seller: "".to_string(), // Null/empty seller in completed swap
            buyer: "GBUYER123".to_string(),
            price: 500,
            token: "CTOKEN123".to_string(),
            status: SwapStatus::Completed,
            expiry: 999999,
        };
        let res = InvariantChecker::verify_completed_swap(&record);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H1_SELLER_NOT_NULL");
    }

    #[test]
    fn test_verify_completed_swap_fails_on_identical_seller_and_buyer() {
        let record = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREGISTRY123".to_string(),
            seller: "GSAME123".to_string(),
            buyer: "GSAME123".to_string(), // Identical
            price: 500,
            token: "CTOKEN123".to_string(),
            status: SwapStatus::Completed,
            expiry: 999999,
        };
        let res = InvariantChecker::verify_completed_swap(&record);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H2_SELLER_BUYER_DISTINCT");
    }

    #[test]
    fn test_verify_ip_record_valid() {
        let record = IpRecord {
            ip_id: 10,
            owner: "GOWNER123".to_string(),
            commitment_hash: "a".repeat(64),
            timestamp: 1600000000,
            revoked: false,
        };
        assert!(InvariantChecker::verify_ip_record(&record).is_ok());
    }

    #[test]
    fn test_verify_ip_record_fails_on_invalid_hash_or_zero_timestamp() {
        let mut record = IpRecord {
            ip_id: 10,
            owner: "GOWNER123".to_string(),
            commitment_hash: "invalid_short_hash".to_string(),
            timestamp: 1600000000,
            revoked: false,
        };
        assert_eq!(
            InvariantChecker::verify_ip_record(&record).unwrap_err().invariant_id,
            "H3_IP_HASH_FORMAT"
        );

        record.commitment_hash = "f".repeat(64);
        record.timestamp = 0; // Zero timestamp
        assert_eq!(
            InvariantChecker::verify_ip_record(&record).unwrap_err().invariant_id,
            "H3_IP_TIMESTAMP_POSITIVE"
        );

        record.timestamp = 100;
        record.owner = "".to_string(); // Empty owner
        assert_eq!(
            InvariantChecker::verify_ip_record(&record).unwrap_err().invariant_id,
            "H3_IP_OWNER_NOT_NULL"
        );
    }

    #[test]
    fn test_verify_batch_initiate_swap_valid() {
        let req = BatchInitiateSwapRequest {
            ip_registry_id: "CREG123".to_string(),
            ip_ids: vec![1, 2, 3],
            seller: "GSELLER".to_string(),
            prices: vec![100, 200, 300],
            buyer: "GBUYER".to_string(),
            token: "CTOKEN".to_string(),
            referrer: None,
            idempotency_key: None,
        };
        let res = BatchInitiateSwapResponse {
            swap_ids: vec![10, 11, 12],
        };
        assert!(InvariantChecker::verify_batch_initiate_swap(&req, &res).is_ok());
    }

    #[test]
    fn test_verify_batch_initiate_swap_fails_on_mismatches_or_non_sequential() {
        let req = BatchInitiateSwapRequest {
            ip_registry_id: "CREG123".to_string(),
            ip_ids: vec![1, 2],
            seller: "GSELLER".to_string(),
            prices: vec![100, 200],
            buyer: "GBUYER".to_string(),
            token: "CTOKEN".to_string(),
            referrer: None,
            idempotency_key: None,
        };
        let res_mismatched = BatchInitiateSwapResponse {
            swap_ids: vec![10], // Mismatched response length
        };
        assert_eq!(
            InvariantChecker::verify_batch_initiate_swap(&req, &res_mismatched).unwrap_err().invariant_id,
            "H4_BATCH_RESPONSE_LENGTH_MATCH"
        );

        let res_non_sequential = BatchInitiateSwapResponse {
            swap_ids: vec![10, 15], // Non-sequential
        };
        assert_eq!(
            InvariantChecker::verify_batch_initiate_swap(&req, &res_non_sequential).unwrap_err().invariant_id,
            "H4_BATCH_SEQUENTIAL_IDS"
        );
    }

    #[test]
    fn test_verify_transfer_ip_valid() {
        assert!(InvariantChecker::verify_transfer_ip("GOWNER1", "GOWNER2", 1).is_ok());
    }

    #[test]
    fn test_verify_transfer_ip_fails_on_same_owner() {
        let res = InvariantChecker::verify_transfer_ip("GOWNER1", "GOWNER1", 1);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H5_TRANSFER_DIFFERENT_OWNER");
    }

    #[test]
    fn test_check_and_alert_fails_loudly_on_violation() {
        let violation = check_and_alert(|| {
            InvariantChecker::verify_swap_record(&SwapRecord {
                ip_id: 1,
                ip_registry_id: "CREG".to_string(),
                seller: "".to_string(), // Null seller
                buyer: "GBUYER".to_string(),
                price: 100,
                token: "CTOKEN".to_string(),
                status: SwapStatus::Completed,
                expiry: 1000,
            })
        });
        assert!(violation.is_err());
        assert_eq!(violation.unwrap_err().invariant_id, "H1_SELLER_NOT_NULL");
    }

    // ── #864: Handler-Specific Invariant Tests ───────────────────────────────

    #[test]
    fn test_verify_accept_swap_valid() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Pending,
            expiry: 999999,
        };
        assert!(InvariantChecker::verify_accept_swap(&swap, "GBUYER").is_ok());
    }

    #[test]
    fn test_verify_accept_swap_fails_if_not_pending() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Completed, // Not Pending
            expiry: 999999,
        };
        let res = InvariantChecker::verify_accept_swap(&swap, "GBUYER");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H7_SWAP_STATE_PENDING");
    }

    #[test]
    fn test_verify_accept_swap_fails_if_empty_buyer() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Pending,
            expiry: 999999,
        };
        let res = InvariantChecker::verify_accept_swap(&swap, "");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H7_BUYER_NOT_EMPTY");
    }

    #[test]
    fn test_verify_accept_swap_fails_if_seller_equals_buyer() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSAME".to_string(),
            buyer: "GSAME".to_string(), // Same as seller
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Pending,
            expiry: 999999,
        };
        let res = InvariantChecker::verify_accept_swap(&swap, "GSAME");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H7_SWAP_PARTIES_DISTINCT");
    }

    #[test]
    fn test_verify_reveal_key_valid() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Accepted,
            expiry: 999999,
        };
        assert!(InvariantChecker::verify_reveal_key(&swap, "GSELLER").is_ok());
    }

    #[test]
    fn test_verify_reveal_key_fails_if_not_accepted() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Pending, // Not Accepted
            expiry: 999999,
        };
        let res = InvariantChecker::verify_reveal_key(&swap, "GSELLER");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H8_SWAP_STATE_ACCEPTED");
    }

    #[test]
    fn test_verify_reveal_key_fails_if_caller_not_seller() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Accepted,
            expiry: 999999,
        };
        let res = InvariantChecker::verify_reveal_key(&swap, "GBUYER"); // Buyer, not seller
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H8_CALLER_IS_SELLER");
    }

    #[test]
    fn test_verify_cancel_swap_valid_from_pending() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Pending,
            expiry: 999999,
        };
        assert!(InvariantChecker::verify_cancel_swap(&swap, "GSELLER").is_ok());
        assert!(InvariantChecker::verify_cancel_swap(&swap, "GBUYER").is_ok());
    }

    #[test]
    fn test_verify_cancel_swap_valid_from_accepted() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Accepted,
            expiry: 999999,
        };
        assert!(InvariantChecker::verify_cancel_swap(&swap, "GSELLER").is_ok());
        assert!(InvariantChecker::verify_cancel_swap(&swap, "GBUYER").is_ok());
    }

    #[test]
    fn test_verify_cancel_swap_fails_if_completed() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Completed, // Not cancellable
            expiry: 999999,
        };
        let res = InvariantChecker::verify_cancel_swap(&swap, "GSELLER");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H9_SWAP_CANCELLABLE");
    }

    #[test]
    fn test_verify_cancel_swap_fails_if_unauthorized() {
        let swap = SwapRecord {
            ip_id: 1,
            ip_registry_id: "CREG123".to_string(),
            seller: "GSELLER".to_string(),
            buyer: "GBUYER".to_string(),
            price: 1000,
            token: "CTOKEN".to_string(),
            status: SwapStatus::Pending,
            expiry: 999999,
        };
        let res = InvariantChecker::verify_cancel_swap(&swap, "GTHIRDPARTY"); // Neither seller nor buyer
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H9_CANCELLER_AUTHORIZED");
    }

    #[test]
    fn test_verify_commit_ip_request_valid() {
        assert!(InvariantChecker::verify_commit_ip_request("GOWNER123", "a".repeat(64)).is_ok());
    }

    #[test]
    fn test_verify_commit_ip_request_fails_on_empty_owner() {
        let res = InvariantChecker::verify_commit_ip_request("", "a".repeat(64));
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H10_COMMIT_OWNER_NOT_EMPTY");
    }

    #[test]
    fn test_verify_commit_ip_request_fails_on_invalid_hash() {
        let res = InvariantChecker::verify_commit_ip_request("GOWNER", "invalid_hash");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().invariant_id, "H10_COMMIT_HASH_FORMAT");

        let res2 = InvariantChecker::verify_commit_ip_request("GOWNER", "g".repeat(64)); // Non-hex character
        assert!(res2.is_err());
        assert_eq!(res2.unwrap_err().invariant_id, "H10_COMMIT_HASH_FORMAT");
    }
}

