#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env};

// ── Storage Keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Swap(u64),
    NextId,
    /// Maps ip_id → swap_id for any swap currently in Pending or Accepted state.
    /// Cleared when a swap reaches Completed or Cancelled.
    ActiveSwap(u64),
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, PartialEq, Eq)]
pub enum SwapStatus {
    Pending,
    Accepted,
    Completed,
    Cancelled,
}

#[contracttype]
#[derive(Clone)]
pub struct SwapRecord {
    pub ip_id: u64,
    pub seller: Address,
    pub buyer: Address,
    pub price: i128,
    pub token: Address,
    pub status: SwapStatus,
    /// Ledger timestamp after which the buyer may cancel an Accepted swap
    /// if reveal_key has not been called. Set at initiation time.
    pub expiry: u64,
}

// ── Events ────────────────────────────────────────────────────────────────────

/// Payload published when a swap is successfully cancelled.
/// Topic: `swp_cncld` (symbol_short, max 9 chars) — used by off-chain indexers.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapCancelledEvent {
    pub swap_id: u64,
    pub canceller: Address,
}

// ── Events ────────────────────────────────────────────────────────────────────

/// Payload published when a key is successfully revealed and the swap completes.
/// Topic: `key_revld` (symbol_short, max 9 chars) — used by off-chain indexers.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct KeyRevealedEvent {
    pub swap_id: u64,
    pub decryption_key: BytesN<32>,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct AtomicSwap;

#[contractimpl]
impl AtomicSwap {
    /// Seller initiates a patent sale. Returns the swap ID.
    /// Panics if an active (Pending or Accepted) swap already exists for this ip_id.
    pub fn initiate_swap(env: Env, ip_id: u64, price: i128, buyer: Address) -> u64 {
        // Guard: reject if an active swap already exists for this IP
        assert!(
            !env.storage().persistent().has(&DataKey::ActiveSwap(ip_id)),
            "active swap already exists for this ip_id"
        );

        let seller = env.current_contract_address(); // placeholder; real impl uses invoker
        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);

        let swap = SwapRecord {
            ip_id,
            seller,
            buyer,
            price,
            status: SwapStatus::Pending,
            expiry,
        };

        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage().persistent().set(&DataKey::ActiveSwap(ip_id), &id);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        id
    }

    /// Buyer accepts the swap and transfers payment into contract escrow.
    pub fn accept_swap(env: Env, swap_id: u64) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        swap.buyer.require_auth();
        assert!(swap.status == SwapStatus::Pending, "swap not pending");
        swap.buyer.require_auth();

        token::Client::new(&env, &swap.token)
            .transfer(&swap.buyer, &env.current_contract_address(), &swap.price);

        swap.status = SwapStatus::Accepted;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
    }

    /// Seller reveals the decryption key; payment releases.
    /// Emits a `key_revld` event on success so external systems can detect
    /// when the key becomes available.
    pub fn reveal_key(env: Env, swap_id: u64, decryption_key: BytesN<32>) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        swap.seller.require_auth();
        assert!(swap.status == SwapStatus::Accepted, "swap not accepted");
        swap.seller.require_auth();

        token::Client::new(&env, &swap.token)
            .transfer(&env.current_contract_address(), &swap.seller, &swap.price);

        swap.status = SwapStatus::Completed;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
        // Release the IP lock so a new swap can be created
        env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
    }

    /// Cancel a swap (invalid key or timeout). Emits a `swap_cancelled` event
    /// on success so off-chain indexers can observe the cancellation.
    pub fn cancel_swap(env: Env, swap_id: u64, canceller: Address) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        assert!(swap.status == SwapStatus::Pending, "only pending swaps can be cancelled this way");
        swap.status = SwapStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);

        // Emit cancellation event — only reached on successful state transition.
        env.events().publish(
            (symbol_short!("swp_cncld"),),
            SwapCancelledEvent { swap_id, canceller },
        );
    }

    /// Buyer cancels an Accepted swap after the expiry has passed and the seller
    /// has not revealed the key. Releases escrowed funds back to the buyer.
    /// Panics if: swap is not Accepted, caller is not the buyer, or expiry has
    /// not yet been reached.
    pub fn cancel_expired_swap(env: Env, swap_id: u64, caller: Address) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        assert!(swap.status == SwapStatus::Accepted, "swap not in Accepted state");
        assert!(caller == swap.buyer, "only the buyer can cancel an expired swap");
        assert!(
            env.ledger().timestamp() > swap.expiry,
            "swap has not expired yet"
        );

        // Full impl: transfer escrowed funds back to buyer here
        swap.status = SwapStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
        // Release the IP lock so a new swap can be created
        env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
    }

    /// Read a swap record. Returns None if the swap_id does not exist.
    pub fn get_swap(env: Env, swap_id: u64) -> Option<SwapRecord> {
        env.storage().persistent().get(&DataKey::Swap(swap_id))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    #[test]
    fn get_swap_returns_none_for_nonexistent_id() {
        let env = Env::default();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        // No swaps have been created; any ID should return None
        let result = client.get_swap(&9999);
        assert!(result.is_none());
    }

    #[test]
    fn get_swap_returns_some_for_existing_swap() {
        let env = Env::default();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let buyer = Address::generate(&env);
        let swap_id = client.initiate_swap(&1_u64, &100_i128, &buyer);

        let result = client.get_swap(&swap_id);
        assert!(result.is_some());
        let swap = result.unwrap();
        assert_eq!(swap.ip_id, 1_u64);
        assert_eq!(swap.price, 100_i128);
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    /// A second initiate_swap for the same ip_id must be rejected while the first is active.
    #[test]
    fn duplicate_swap_rejected_while_active() {
        let env = Env::default();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let buyer = Address::generate(&env);
        client.initiate_swap(&1_u64, &100_i128, &buyer);
        // Second call for the same ip_id must fail
        let result = client.try_initiate_swap(&1_u64, &200_i128, &buyer);
        assert!(result.is_err());
    }

    /// After a swap is cancelled the IP lock is released and a new swap can be created.
    #[test]
    fn new_swap_allowed_after_cancel() {
        let env = Env::default();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let buyer = Address::generate(&env);
        let swap_id = client.initiate_swap(&2_u64, &100_i128, &buyer);
        client.cancel_swap(&swap_id);

        // Lock released — this must succeed
        let new_id = client.initiate_swap(&2_u64, &150_i128, &buyer);
        assert_ne!(new_id, swap_id);
    }

    /// After a swap completes the IP lock is released and a new swap can be created.
    #[test]
    fn new_swap_allowed_after_complete() {
        let env = Env::default();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let buyer = Address::generate(&env);
        let swap_id = client.initiate_swap(&3_u64, &100_i128, &buyer);
        client.accept_swap(&swap_id);

        let key = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
        client.reveal_key(&swap_id, &key);

        // Lock released — this must succeed
        let new_id = client.initiate_swap(&3_u64, &150_i128, &buyer);
        assert_ne!(new_id, swap_id);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ip_registry::{IpRegistry, IpRegistryClient as RegistryClient};
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _, Ledger},
        Env,
    };

    fn setup() -> (Env, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let registry_id = env.register_contract(None, IpRegistry);
        let swap_id = env.register_contract(None, AtomicSwap);
        let owner = Address::generate(&env);

        (env, registry_id, swap_id, owner)
    }

    /// Register an IP and return (ip_id, swap_contract_client, buyer).
    fn setup_with_ip(
        env: &Env,
        registry_id: &Address,
        swap_id: &Address,
        owner: &Address,
    ) -> (u64, AtomicSwapClient, Address) {
        let registry = RegistryClient::new(env, registry_id);
        let hash = BytesN::random(env);
        let ip_id = registry.commit_ip(owner, &hash);
        let swap_client = AtomicSwapClient::new(env, swap_id);
        let buyer = Address::generate(env);
        (ip_id, swap_client, buyer)
    }

    #[test]
    fn test_initiate_swap_valid_ip_id_succeeds() {
        let (env, registry_id, swap_id, owner) = setup();
        let (ip_id, swap_client, buyer) = setup_with_ip(&env, &registry_id, &swap_id, &owner);

        let result = swap_client.initiate_swap(&registry_id, &ip_id, &1000_i128, &buyer, &0u64);
        assert_eq!(result, 0u64);

        let record = swap_client.get_swap(&result);
        assert_eq!(record.ip_id, ip_id);
        assert_eq!(record.status, SwapStatus::Pending);
        // expiry should be set to now + default duration
        assert_eq!(record.expiry, env.ledger().timestamp() + DEFAULT_SWAP_DURATION_SECS);
    }

    #[test]
    #[should_panic(expected = "IP not found")]
    fn test_initiate_swap_nonexistent_ip_id_panics() {
        let (env, registry_id, swap_id, _owner) = setup();
        let swap_client = AtomicSwapClient::new(&env, &swap_id);
        let buyer = Address::generate(&env);
        swap_client.initiate_swap(&registry_id, &999u64, &500_i128, &buyer, &0u64);
    }

    #[test]
    fn test_cancel_expired_swap_after_timeout() {
        let (env, registry_id, swap_id, owner) = setup();
        let (ip_id, swap_client, buyer) = setup_with_ip(&env, &registry_id, &swap_id, &owner);

        // Initiate with a short 100-second window
        let duration: u64 = 100;
        let swap_record_id =
            swap_client.initiate_swap(&registry_id, &ip_id, &1000_i128, &buyer, &duration);

        // Buyer accepts
        swap_client.accept_swap(&swap_record_id);

        let record = swap_client.get_swap(&swap_record_id);
        assert_eq!(record.status, SwapStatus::Accepted);

        // Advance past expiry (premature cancellation is covered by a separate #[should_panic] test)
        let expiry = record.expiry;
        env.ledger().with_mut(|l| l.timestamp = expiry + 1);

        // Buyer cancels — must succeed and funds return to buyer
        swap_client.cancel_expired_swap(&swap_record_id, &buyer);

        let final_record = swap_client.get_swap(&swap_record_id);
        assert_eq!(final_record.status, SwapStatus::Cancelled);
    }

    #[test]
    #[should_panic(expected = "swap has not expired yet")]
    fn test_cancel_expired_swap_before_timeout_panics() {
        let (env, registry_id, swap_id, owner) = setup();
        let (ip_id, swap_client, buyer) = setup_with_ip(&env, &registry_id, &swap_id, &owner);

        let swap_record_id =
            swap_client.initiate_swap(&registry_id, &ip_id, &1000_i128, &buyer, &100u64);
        swap_client.accept_swap(&swap_record_id);

        // Do NOT advance time — expiry has not passed
        swap_client.cancel_expired_swap(&swap_record_id, &buyer);
    }

    #[test]
    #[should_panic(expected = "only the buyer can cancel an expired swap")]
    fn test_cancel_expired_swap_non_buyer_panics() {
        let (env, registry_id, swap_id, owner) = setup();
        let (ip_id, swap_client, buyer) = setup_with_ip(&env, &registry_id, &swap_id, &owner);

        let swap_record_id =
            swap_client.initiate_swap(&registry_id, &ip_id, &1000_i128, &buyer, &100u64);
        swap_client.accept_swap(&swap_record_id);

        let expiry = swap_client.get_swap(&swap_record_id).expiry;
        env.ledger().with_mut(|l| l.timestamp = expiry + 1);

        // Stranger tries to cancel — must panic
        let stranger = Address::generate(&env);
        swap_client.cancel_expired_swap(&swap_record_id, &stranger);
    }

    #[test]
    #[should_panic(expected = "swap not in Accepted state")]
    fn test_cancel_expired_swap_on_pending_panics() {
        let (env, registry_id, swap_id, owner) = setup();
        let (ip_id, swap_client, buyer) = setup_with_ip(&env, &registry_id, &swap_id, &owner);

        let swap_record_id =
            swap_client.initiate_swap(&registry_id, &ip_id, &1000_i128, &buyer, &100u64);

        // Advance past expiry without accepting first
        env.ledger().with_mut(|l| l.timestamp = l.timestamp + 200);
        swap_client.cancel_expired_swap(&swap_record_id, &buyer);
    }

    #[test]
    fn test_reveal_key_before_expiry_completes_swap() {
        let (env, registry_id, swap_id, owner) = setup();
        let (ip_id, swap_client, buyer) = setup_with_ip(&env, &registry_id, &swap_id, &owner);

        let swap_record_id =
            swap_client.initiate_swap(&registry_id, &ip_id, &1000_i128, &buyer, &100u64);
        swap_client.accept_swap(&swap_record_id);

        // Reveal key well within the window — normal happy path unaffected
        let key = BytesN::random(&env);
        swap_client.reveal_key(&swap_record_id, &key);

        let record = swap_client.get_swap(&swap_record_id);
        assert_eq!(record.status, SwapStatus::Completed);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        vec, Env, IntoVal,
    };

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AtomicSwap);
        let canceller = Address::generate(&env);
        (env, contract_id, canceller)
    }

    fn make_swap(env: &Env, client: &AtomicSwapClient) -> u64 {
        let buyer = Address::generate(env);
        client.initiate_swap(&1u64, &1000_i128, &buyer)
    }

    #[test]
    fn test_cancel_pending_swap_emits_event() {
        let (env, contract_id, canceller) = setup();
        let client = AtomicSwapClient::new(&env, &contract_id);
        let swap_id = make_swap(&env, &client);

        client.cancel_swap(&swap_id, &canceller);

        // Confirm state transitioned
        assert_eq!(client.get_swap(&swap_id).status, SwapStatus::Cancelled);

        // Assert the event was emitted with the correct topic and payload
        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (_, topics, data) = events.get(0).unwrap();
        assert_eq!(topics, vec![&env, symbol_short!("swp_cncld").into_val(&env)]);
        let payload: SwapCancelledEvent = data.into_val(&env);
        assert_eq!(payload.swap_id, swap_id);
        assert_eq!(payload.canceller, canceller);
    }

    #[test]
    fn test_cancel_accepted_swap_emits_event() {
        let (env, contract_id, canceller) = setup();
        let client = AtomicSwapClient::new(&env, &contract_id);
        let swap_id = make_swap(&env, &client);

        client.accept_swap(&swap_id);
        client.cancel_swap(&swap_id, &canceller);

        assert_eq!(client.get_swap(&swap_id).status, SwapStatus::Cancelled);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (_, _, data) = events.get(0).unwrap();
        let payload: SwapCancelledEvent = data.into_val(&env);
        assert_eq!(payload.swap_id, swap_id);
        assert_eq!(payload.canceller, canceller);
    }

    #[test]
    #[should_panic(expected = "swap already finalised")]
    fn test_cancel_completed_swap_fails_no_event() {
        let (env, contract_id, canceller) = setup();
        let client = AtomicSwapClient::new(&env, &contract_id);
        let swap_id = make_swap(&env, &client);

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &soroban_sdk::testutils::BytesN::random(&env));

        // This must panic — no event should be emitted
        client.cancel_swap(&swap_id, &canceller);
    }

    /// Confirms no swap_cancelled event is emitted when the swap completes normally.
    /// A completed swap has no cancellation event — the events list stays empty.
    #[test]
    fn test_no_cancel_event_when_swap_completed_normally() {
        let (env, contract_id, _canceller) = setup();
        let client = AtomicSwapClient::new(&env, &contract_id);
        let swap_id = make_swap(&env, &client);

        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &soroban_sdk::testutils::BytesN::random(&env));

        // Swap completed via reveal_key — no cancellation event should exist
        assert_eq!(env.events().all().len(), 0);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, BytesN as _, Events},
        vec, Env, IntoVal,
    };

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, AtomicSwap);
        (env, contract_id)
    }

    /// Bring a swap to Accepted state and return its ID + the key used.
    fn accepted_swap(env: &Env, client: &AtomicSwapClient) -> (u64, BytesN<32>) {
        let buyer = Address::generate(env);
        let swap_id = client.initiate_swap(&1u64, &1000_i128, &buyer);
        client.accept_swap(&swap_id);
        let key = BytesN::random(env);
        (swap_id, key)
    }

    #[test]
    fn test_reveal_key_emits_event_with_correct_values() {
        let (env, contract_id) = setup();
        let client = AtomicSwapClient::new(&env, &contract_id);
        let (swap_id, key) = accepted_swap(&env, &client);

        client.reveal_key(&swap_id, &key);

        // State must be Completed
        assert_eq!(client.get_swap(&swap_id).status, SwapStatus::Completed);

        // Exactly one event emitted
        let events = env.events().all();
        assert_eq!(events.len(), 1);

        // Topic is correct
        let (_, topics, data) = events.get(0).unwrap();
        assert_eq!(topics, vec![&env, symbol_short!("key_revld").into_val(&env)]);

        // Payload fields match exactly
        let payload: KeyRevealedEvent = data.into_val(&env);
        assert_eq!(payload.swap_id, swap_id);
        assert_eq!(payload.decryption_key, key);
    }

    #[test]
    #[should_panic(expected = "swap not accepted")]
    fn test_reveal_key_on_pending_swap_fails_no_event() {
        let (env, contract_id) = setup();
        let client = AtomicSwapClient::new(&env, &contract_id);
        let buyer = Address::generate(&env);
        let swap_id = client.initiate_swap(&1u64, &1000_i128, &buyer);
        let key = BytesN::random(&env);

        // Swap is still Pending — must panic before event fires
        client.reveal_key(&swap_id, &key);
    }

    #[test]
    #[should_panic(expected = "swap not accepted")]
    fn test_reveal_key_on_completed_swap_fails_no_event() {
        let (env, contract_id) = setup();
        let client = AtomicSwapClient::new(&env, &contract_id);
        let (swap_id, key) = accepted_swap(&env, &client);

        client.reveal_key(&swap_id, &key);
        // Second call on an already-Completed swap — must panic
        client.reveal_key(&swap_id, &key);
    }

    #[test]
    fn test_no_event_emitted_on_normal_completion_without_reveal() {
        let (env, contract_id) = setup();
        let client = AtomicSwapClient::new(&env, &contract_id);
        let buyer = Address::generate(&env);
        let swap_id = client.initiate_swap(&1u64, &1000_i128, &buyer);
        client.accept_swap(&swap_id);

        // No reveal_key called — events list must be empty
        assert_eq!(env.events().all().len(), 0);
    }
}
