#[cfg(test)]
mod tests {
    use soroban_sdk::{BytesN, Env, Address};
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_basic_functionality() {
        let env = Env::default();

        let owner = Address::generate(&env);
        let commitment_hash = BytesN::from_array(&env, &[0; 32]);

        assert_eq!(commitment_hash.len(), 32);

        let owner2 = Address::generate(&env);
        assert_ne!(owner, owner2);
    }

    #[test]
    fn test_storage_keys() {
        let env = Env::default();

        let key = crate::DataKey::IpRecord(1);
        let key2 = crate::DataKey::IpRecord(2);
        assert_ne!(key, key2);

        let owner_key = crate::DataKey::OwnerIps(Address::generate(&env));
        let next_id_key = crate::DataKey::NextId;
        assert_ne!(owner_key, next_id_key);
    }

    #[test]
    fn test_ip_record_creation() {
        let env = Env::default();

        let owner = Address::generate(&env);
        let commitment_hash = BytesN::from_array(&env, &[1; 32]);
        let timestamp = env.ledger().timestamp();

        let record = crate::IpRecord {
            ip_id: 1,
            owner: owner.clone(),
            commitment_hash,
            timestamp,
            revoked: false,
        };

        assert_eq!(record.owner, owner);
        assert_eq!(record.timestamp, timestamp);
        assert!(!record.revoked);
    }
}
