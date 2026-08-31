#[cfg(test)]
mod arbitration_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, BytesN, Env, Vec,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    const RULING_DELAY: u64 = 48 * 3600;

    // ── #830: Compile-time surface assertion ─────────────────────────────────
    //
    // Verify that the public contract surface used by these tests matches what
    // ip_registry expects.  The assertions below are pure compile-time checks:
    // if the signatures drift the file will not compile.
    //
    //  • `IpRegistryClient::commit_ip` must accept (owner: &Address,
    //    commitment_hash: &BytesN<32>, pow_difficulty: &u32) — three args.
    //  • `AtomicSwapClient::initiate_swap` must accept nine user-visible args
    //    (token, ip_id, seller, price, buyer, required_approvals, referrer,
    //    collateral_amount, insurance_enabled).
    //
    // Both are enforced implicitly throughout this module: every
    // `registry.commit_ip(owner, &hash, &0u32)` call is a three-arg call site,
    // and every `client.initiate_swap(…)` call passes exactly nine arguments.
    // Any future removal of either parameter will break compilation here before
    // it can reach main.
    //
    // Additionally, the removed function `accept_swap_with_quantity` has zero
    // callers in this crate (confirmed by `grep accept_swap_with_quantity
    // contracts/atomic_swap/src/*.rs` returning no results).

    fn setup_registry(env: &Env, owner: &Address) -> (Address, u64, BytesN<32>, BytesN<32>) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let secret = BytesN::from_array(env, &[2u8; 32]);
        let blinding = BytesN::from_array(env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(owner, &commitment_hash, &0u32);
        (registry_id, ip_id, secret, blinding)
    }

    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
    }

    /// Mints enough of the swap token to both buyer and seller to cover the
    /// swap price plus `MIN_DISPUTE_BOND` (10_000_000, #781), so either party
    /// can submit evidence (and pay the resulting bond) in these tests.
    fn setup_disputed_swap(env: &Env) -> (AtomicSwapClient, u64, Address, Address) {
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let token_admin = Address::generate(env);
        let (registry_id, ip_id, _, _) = setup_registry(env, &seller);
        let token_id = setup_token(env, &token_admin, &buyer, 20_000_000);
        StellarAssetClient::new(env, &token_id).mint(&seller, &20_000_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(env, &contract_id);
        client.initialize(&registry_id);

        // Price is kept small (well under 40) so protocol_fee_bps's fee
        // floors to 0 and the "complete to seller" ruling path never has to
        // transfer a fee to protocol_config().treasury — that address is a
        // pre-existing hardcoded placeholder with no trustline for this
        // test's token, a separate storage bug (see docs/threat-model.md's
        // #781 update) this PR does not fix.
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &20_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        client.accept_swap(&swap_id);
        client.raise_dispute(&swap_id);

        (client, swap_id, seller, buyer)
    }

    /// A 3-signer, 2-of-3 committee (the threat model's stated minimum).
    fn committee(env: &Env) -> Vec<Address> {
        let mut signers = Vec::new(env);
        signers.push_back(Address::generate(env));
        signers.push_back(Address::generate(env));
        signers.push_back(Address::generate(env));
        signers
    }

    fn two_of(signers: &Vec<Address>, env: &Env) -> Vec<Address> {
        let mut two = Vec::new(env);
        two.push_back(signers.get(0).unwrap());
        two.push_back(signers.get(1).unwrap());
        two
    }

    fn skip_ruling_delay(env: &Env) {
        env.ledger().with_mut(|l| l.timestamp += RULING_DELAY);
    }

    // ── #781: set_arbitrator (M-of-N committee) ─────────────────────────────

    #[test]
    fn test_set_arbitrator_on_disputed_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);

        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.arbitrator, Some(signers.get(0).unwrap()));
    }

    #[test]
    #[should_panic]
    fn test_set_arbitrator_twice_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);

        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);
        // Second call should panic with ArbitratorAlreadySet
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);
    }

    #[test]
    #[should_panic]
    fn test_set_arbitrator_on_non_disputed_swap_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin_token = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin_token, &buyer, 1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        // Swap is Pending, not Disputed — should panic
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);
    }

    #[test]
    #[should_panic]
    fn test_set_arbitrator_committee_too_small_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let admin = Address::generate(&env);
        // Only 2 signers — below the 2-of-3 minimum committee size.
        let mut signers = Vec::new(&env);
        signers.push_back(Address::generate(&env));
        signers.push_back(Address::generate(&env));

        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);
    }

    #[test]
    #[should_panic]
    fn test_set_arbitrator_duplicate_signer_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let admin = Address::generate(&env);
        let dup = Address::generate(&env);
        let mut signers = Vec::new(&env);
        signers.push_back(dup.clone());
        signers.push_back(dup);
        signers.push_back(Address::generate(&env));

        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);
    }

    // ── #781: arbitrate_dispute (ruling entry) + execute_ruling ─────────────

    #[test]
    fn test_ruling_refunds_buyer_after_delay() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash = BytesN::from_array(&env, &[0xabu8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &true);
        skip_ruling_delay(&env);
        client.execute_ruling(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_ruling_completes_to_seller_after_delay() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, seller, _) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash = BytesN::from_array(&env, &[0xcdu8; 32]);
        client.submit_dispute_evidence(&swap_id, &seller, &hash);

        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &false);
        skip_ruling_delay(&env);
        client.execute_ruling(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn test_ruling_without_evidence_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        // No evidence submitted — should panic with EvidenceRequired.
        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &true);
    }

    #[test]
    #[should_panic]
    fn test_ruling_with_insufficient_signers_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash = BytesN::from_array(&env, &[0x11u8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        let mut one = Vec::new(&env);
        one.push_back(signers.get(0).unwrap());
        // Only 1 of 3 signers, threshold is 2 — should panic InsufficientSignatures.
        client.arbitrate_dispute(&swap_id, &one, &true);
    }

    #[test]
    #[should_panic]
    fn test_ruling_by_non_committee_signer_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash = BytesN::from_array(&env, &[0x22u8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        let mut impostors = Vec::new(&env);
        impostors.push_back(signers.get(0).unwrap());
        impostors.push_back(Address::generate(&env)); // not a committee member
        client.arbitrate_dispute(&swap_id, &impostors, &true);
    }

    #[test]
    #[should_panic]
    fn test_double_pending_ruling_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash = BytesN::from_array(&env, &[0x33u8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &true);
        // A ruling is already pending — should panic RulingAlreadyPending.
        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &true);
    }

    #[test]
    #[should_panic]
    fn test_execute_ruling_before_delay_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash = BytesN::from_array(&env, &[0x44u8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &true);
        // No time skip — should panic TimelockNotElapsed.
        client.execute_ruling(&swap_id);
    }

    #[test]
    fn test_cancel_pending_ruling_within_window() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash = BytesN::from_array(&env, &[0x55u8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &true);
        client.cancel_pending_ruling(&swap_id, &two_of(&signers, &env));

        // A fresh ruling can be entered after cancellation.
        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &false);
        skip_ruling_delay(&env);
        client.execute_ruling(&swap_id);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Completed);
    }

    #[test]
    #[should_panic]
    fn test_cancel_pending_ruling_after_window_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash = BytesN::from_array(&env, &[0x66u8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &true);
        skip_ruling_delay(&env);
        // Window closed — should panic RulingFinalized.
        client.cancel_pending_ruling(&swap_id, &two_of(&signers, &env));
    }

    // ── #781: dispute bond ────────────────────────────────────────────────────

    #[test]
    fn test_bond_charged_once_per_submitter() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let hash1 = BytesN::from_array(&env, &[0x77u8; 32]);
        let hash2 = BytesN::from_array(&env, &[0x78u8; 32]);

        client.submit_dispute_evidence(&swap_id, &buyer, &hash1);
        // Second submission by the same buyer must not re-charge the bond —
        // asserted indirectly: this must not panic on insufficient balance
        // even though the buyer only started with 20_000_000.
        client.submit_dispute_evidence(&swap_id, &buyer, &hash2);

        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 2);
    }

    #[test]
    fn test_winning_bond_refunded_losing_bond_forfeited() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, seller, buyer) = setup_disputed_swap(&env);
        let signers = committee(&env);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.set_arbitrator(&swap_id, &admin, &signers, &2u32);

        let hash1 = BytesN::from_array(&env, &[0x81u8; 32]);
        let hash2 = BytesN::from_array(&env, &[0x82u8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash1);
        client.submit_dispute_evidence(&swap_id, &seller, &hash2);

        // Ruling favors the buyer (refund=true): buyer's bond is refunded,
        // seller's bond is forfeited to the admin.
        client.arbitrate_dispute(&swap_id, &two_of(&signers, &env), &true);
        skip_ruling_delay(&env);
        client.execute_ruling(&swap_id);

        let token_client =
            soroban_sdk::token::Client::new(&env, &client.get_swap(&swap_id).unwrap().token);
        // Buyer paid price(20) + bond(10_000_000) and, having won the
        // ruling, gets both back in full: fully whole again at 20_000_000.
        assert_eq!(token_client.balance(&buyer), 20_000_000);
        // Seller's bond (10_000_000 of their 20_000_000) was forfeited;
        // seller never received the price either way (buyer won).
        assert_eq!(token_client.balance(&seller), 20_000_000 - 10_000_000);
    }

    #[test]
    fn test_uncontested_resolution_refunds_outstanding_bond() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let hash = BytesN::from_array(&env, &[0x91u8; 32]);
        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        let token_client =
            soroban_sdk::token::Client::new(&env, &client.get_swap(&swap_id).unwrap().token);
        assert_eq!(token_client.balance(&buyer), 20_000_000 - 20 - 10_000_000);

        // resolve_dispute bypasses the committee ruling flow entirely — the
        // outstanding bond must be refunded in full, not orphaned.
        let admin_caller = Address::generate(&env);
        env.as_contract(&client.address, || {
            env.storage()
                .instance()
                .set(&crate::DataKey::Admin, &admin_caller);
        });
        client.resolve_dispute(&swap_id, &admin_caller, &true);

        // Price and bond both refunded — fully whole again.
        assert_eq!(token_client.balance(&buyer), 20_000_000);
    }

    // ── #313: submit_dispute_evidence ────────────────────────────────────────

    #[test]
    fn test_buyer_can_submit_evidence() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, buyer) = setup_disputed_swap(&env);
        let hash = BytesN::from_array(&env, &[0xabu8; 32]);

        client.submit_dispute_evidence(&swap_id, &buyer, &hash);

        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence.get(0).unwrap(), hash);
    }

    #[test]
    fn test_seller_can_submit_evidence() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, seller, _) = setup_disputed_swap(&env);
        let hash = BytesN::from_array(&env, &[0xbbu8; 32]);

        client.submit_dispute_evidence(&swap_id, &seller, &hash);

        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 1);
    }

    #[test]
    fn test_multiple_evidence_submissions_accumulate() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, seller, buyer) = setup_disputed_swap(&env);
        let hash1 = BytesN::from_array(&env, &[0x01u8; 32]);
        let hash2 = BytesN::from_array(&env, &[0x02u8; 32]);

        client.submit_dispute_evidence(&swap_id, &buyer, &hash1);
        client.submit_dispute_evidence(&swap_id, &seller, &hash2);

        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 2);
    }

    #[test]
    #[should_panic]
    fn test_third_party_cannot_submit_evidence() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let outsider = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xffu8; 32]);

        // outsider is neither buyer nor seller — should panic
        client.submit_dispute_evidence(&swap_id, &outsider, &hash);
    }

    #[test]
    fn test_get_dispute_evidence_empty_for_new_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, swap_id, _, _) = setup_disputed_swap(&env);
        let evidence = client.get_dispute_evidence(&swap_id);
        assert_eq!(evidence.len(), 0);
    }

    // ── accept_swap_partial ───────────────────────────────────────────────────

    #[test]
    fn test_accept_swap_partial_proportional_price() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // Initiate with price=1000, default quantity=1 — set quantity via initiate_swap
        // then manually bump quantity to 10 by accepting partial
        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &1000_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // Patch quantity to 10 so partial acceptance makes sense
        let mut swap = client.get_swap(&swap_id).unwrap();
        swap.quantity = 10;
        // Save via storage directly in test env
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&crate::DataKey::Swap(swap_id), &swap);
        });

        // Accept 3 out of 10 → price = 1000 * 3 / 10 = 300
        client.accept_swap_partial(&swap_id, &3_u32);

        let accepted = client.get_swap(&swap_id).unwrap();
        assert_eq!(accepted.status, SwapStatus::Accepted);
        assert_eq!(accepted.price, 300);
        assert_eq!(accepted.quantity, 3);
    }

    #[test]
    fn test_accept_swap_partial_full_quantity_equals_accept() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );

        // quantity=1 (default), accepting 1/1 = full price
        client.accept_swap_partial(&swap_id, &1_u32);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.price, 500);
    }

    #[test]
    #[should_panic]
    fn test_accept_swap_partial_zero_quantity_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        client.accept_swap_partial(&swap_id, &0_u32);
    }

    #[test]
    #[should_panic]
    fn test_accept_swap_partial_exceeds_quantity_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id, _, _) = setup_registry(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 10_000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap(
            &token_id, &ip_id, &seller, &500_i128, &buyer, &0_u32, &None, &0_i128, &false,
        );
        // quantity=1 by default, requesting 2 should panic
        client.accept_swap_partial(&swap_id, &2_u32);
    }

    // ── #781: batch_arbitrate_swaps (disabled) ──────────────────────────────
    //
    // batch_arbitrate_swaps never validated its `arbitrator` argument against
    // any stored arbitrator/committee — any caller could drain any disputed
    // swap through it, fully bypassing the M-of-N committee/evidence/
    // timelock/bond system added in #781. It is now disabled unconditionally
    // pending a follow-up migration onto the committee model.

    #[test]
    #[should_panic]
    fn test_batch_arbitrate_swaps_disabled() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let arbitrator = Address::generate(&env);
        let token_admin = Address::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let hash1 = BytesN::from_array(&env, &[0x01u8; 32]);
        let hash2 = BytesN::from_array(&env, &[0x02u8; 32]);
        let ip1 = registry.commit_ip(&seller, &hash1, &0u32);
        let ip2 = registry.commit_ip(&seller, &hash2, &0u32);

        let token_id = setup_token(&env, &token_admin, &buyer, 10_000_000);
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let mut ip_ids = Vec::new(&env);
        ip_ids.push_back(ip1);
        ip_ids.push_back(ip2);
        let mut prices = Vec::new(&env);
        prices.push_back(20i128);
        prices.push_back(30i128);

        let swap_ids =
            client.batch_initiate_swap(&token_id, &ip_ids, &seller, &prices, &buyer, &0u32, &None);
        client.batch_accept_swaps(&swap_ids, &buyer);
        client.raise_dispute(&swap_ids.get(0).unwrap());
        client.raise_dispute(&swap_ids.get(1).unwrap());

        // Disabled — must always panic, regardless of caller or dispute state.
        client.batch_arbitrate_swaps(&swap_ids, &arbitrator, &true);
    }
}
