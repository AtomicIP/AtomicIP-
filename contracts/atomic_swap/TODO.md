# Atomic Swap TODO

## Active: Dispute Window Implementation (#68)

**Status:** Approved plan - implementing.

### Detailed Steps:
- [ ] **Step 1:** Update `src/lib.rs`
  - Add ContractError: DisputeWindowExpired=17, OnlyBuyerCanDispute=18, SwapNotDisputed=19, OnlyAdminCanResolve=20
  - Add `Disputed` to `SwapStatus`
  - Add `accept_timestamp: u64` to `SwapRecord` (set on accept_swap)
  - Add `dispute_window_seconds: u64` to `ProtocolConfig` (default 86400)
  - Add events: `DisputeRaised {swap_id: u64}`, `DisputeResolved {swap_id: u64, refunded: bool}`
  - Add `admin_set_protocol_config(env, fee_bps, dispute_window_seconds, treasury)`
  - Add `dispute_reveal(env, swap_id)`: buyer-only, status==Accepted, ledger.ts - swap.accept_timestamp < config.dispute_window → Disputed, emit
  - Add `resolve_dispute(env, swap_id, refunded: bool)`: admin-only, status==Disputed:
    | refunded=true: Cancelled, refund buyer full price, clear ActiveSwap
    | false: Completed, transfer to seller w/fee (no key verify)
  - Update `reveal_key`: reject if Disputed
  - Update `cancel_expired_swap`: reject if Disputed
  - On Completed/Cancelled/Disputed→resolved: clear SellerSwaps/BuyerSwaps lists? (remove id from vec - complex, skip for v1 or implement).
- [ ] **Step 2:** Add tests to `src/basic_tests.rs`
  - test_dispute_after_accept_within_window_succeeds
  - test_dispute_expired_rejected
  - test_resolve_refund_buyer
  - test_resolve_complete_seller
  - test_reveal_rejected_when_disputed
  - test_cancel_expired_rejected_when_disputed
  - test_admin_set_config
- [ ] **Step 3:** `cd contracts/atomic_swap && cargo test`
- [ ] **Step 4:** Mark complete, update TODO.md

**Notes:** Dispute window from accept_timestamp (more precise). Lists cleanup later.

## Previous: Protocol Fee (Done)
...


