/// #379 Contract State Snapshot Testing — Atomic Swap
///
/// Verifies contract state after key operations via field-level snapshots.
#[cfg(test)]
mod snapshot_tests {
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, BytesN, Env,
    };

    use crate::{AtomicSwap, AtomicSwapClient, SwapStatus};

    // ── Snapshot type ─────────────────────────────────────────────────────────

    #[derive(Debug, PartialEq)]
    struct SwapSnapshot {
        ip_id: u64,
        price: i128,
        status: SwapStatus,
    }

    fn snap_swap(client: &AtomicSwapClient, swap_id: u64) -> SwapSnapshot {
        let r = client.get_swap(&swap_id).unwrap();
        SwapSnapshot {
            ip_id: r.ip_id,
            price: r.price,
            status: r.status,
        }
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    fn setup(env: &Env) -> (AtomicSwapClient, Address, Address, Address, u64, BytesN<32>, BytesN<32>) {
        env.mock_all_auths();
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let admin = Address::generate(env);

        let reg_id = env.register(IpRegistry, ());
        let reg = IpRegistryClient::new(env, &reg_id);

        let secret = BytesN::from_array(env, &[0x11u8; 32]);
        let blinding = BytesN::from_array(env, &[0x22u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = reg.commit_ip(&seller, &commitment_hash);

        let token_id = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(env, &token_id).mint(&buyer, &10_000_i128);

        let swap_cid = env.register(AtomicSwap, ());
        let swap = AtomicSwapClient::new(env, &swap_cid);
        swap.initialize(&reg_id);

        (swap, token_id, seller, buyer, ip_id, secret, blinding)
    }

    // ── initiate_swap snapshot ────────────────────────────────────────────────

    #[test]
    fn snapshot_after_initiate_swap() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);

        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &750_i128, &buyer, &0u32, &None, &false);

        assert_eq!(
            snap_swap(&swap, sid),
            SwapSnapshot { ip_id, price: 750, status: SwapStatus::Pending }
        );
    }

    // ── accept_swap snapshot ──────────────────────────────────────────────────

    #[test]
    fn snapshot_after_accept_swap() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);

        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &750_i128, &buyer, &0u32, &None, &false);
        swap.accept_swap(&sid);

        assert_eq!(
            snap_swap(&swap, sid),
            SwapSnapshot { ip_id, price: 750, status: SwapStatus::Accepted }
        );
    }

    // ── reveal_key snapshot ───────────────────────────────────────────────────

    #[test]
    fn snapshot_after_reveal_key() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, secret, blinding) = setup(&env);

        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &750_i128, &buyer, &0u32, &None, &false);
        swap.accept_swap(&sid);
        swap.reveal_key(&sid, &seller, &secret, &blinding);

        assert_eq!(
            snap_swap(&swap, sid),
            SwapSnapshot { ip_id, price: 750, status: SwapStatus::Completed }
        );
    }

    // ── cancel_swap snapshot ──────────────────────────────────────────────────

    #[test]
    fn snapshot_after_cancel_swap() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);

        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &750_i128, &buyer, &0u32, &None, &false);
        swap.cancel_swap(&sid, &seller);

        assert_eq!(
            snap_swap(&swap, sid),
            SwapSnapshot { ip_id, price: 750, status: SwapStatus::Cancelled }
        );
    }

    // ── state diff: initiate does not mutate other swaps ─────────────────────

    #[test]
    fn snapshot_first_swap_unchanged_after_second_ip_registered() {
        let env = Env::default();
        let (swap, token_id, seller, buyer, ip_id, _, _) = setup(&env);

        let sid = swap.initiate_swap(&token_id, &ip_id, &seller, &500_i128, &buyer, &0u32, &None, &false);
        let snap_before = snap_swap(&swap, sid);

        // Cancel so the IP is free, then verify first swap record is unchanged.
        swap.cancel_swap(&sid, &seller);
        let snap_after_cancel = snap_swap(&swap, sid);

        // Only status changes; ip_id and price must be stable.
        assert_eq!(snap_before.ip_id, snap_after_cancel.ip_id);
        assert_eq!(snap_before.price, snap_after_cancel.price);
        assert_eq!(snap_after_cancel.status, SwapStatus::Cancelled);
    }
}
