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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
