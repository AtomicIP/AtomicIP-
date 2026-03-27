#![no_std]
use ip_registry::IpRegistryClient;
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Error};

// ── Error Codes ────────────────────────────────────────────────────────────

#[repr(u32)]
pub enum ContractError {
    SwapNotFound = 1,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Debug, PartialEq)]
pub enum DataKey {
    Swap(u64),
    NextId,
    /// Maps ip_id → swap_id for any swap currently in Pending or Accepted state.
    /// Cleared when a swap reaches Completed or Cancelled.
    ActiveSwap(u64),
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
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
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapCancelledEvent {
    pub swap_id: u64,
    pub canceller: Address,
}

/// Payload published when a key is successfully revealed and the swap completes.
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
    pub fn initiate_swap(
        env: Env,
        ip_registry_id: Address,
        ip_id: u64,
        seller: Address,
        price: i128,
        buyer: Address,
    ) -> u64 {
        seller.require_auth();

        let registry = IpRegistryClient::new(&env, &ip_registry_id);
        let record = registry.get_ip(&ip_id);
        assert!(record.owner == seller, "seller is not the IP owner");

        assert!(
            !env.storage().persistent().has(&DataKey::ActiveSwap(ip_id)),
            "active swap already exists for this ip_id"
        );

        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);

        let swap = SwapRecord {
            ip_id,
            seller,
            buyer,
            price,
            token: env.current_contract_address(), // placeholder
            status: SwapStatus::Pending,
            expiry: env.ledger().timestamp() + 86400,
        };

        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(id), 50000, 50000);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        id
    }

    /// Buyer accepts the swap.
    pub fn accept_swap(env: Env, swap_id: u64) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ))
            });

        swap.buyer.require_auth();
        assert!(swap.status == SwapStatus::Pending, "swap not pending");

        swap.status = SwapStatus::Accepted;
        env.storage()
            .persistent()
            .set(&DataKey::Swap(swap_id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);
    }

    /// Seller reveals the decryption key; payment releases.
    /// SECURITY: caller must be the seller — verified by identity check + require_auth.
    pub fn reveal_key(env: Env, swap_id: u64, caller: Address, _decryption_key: BytesN<32>) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ))
            });

        assert!(caller == swap.seller, "only the seller can reveal the key");
        caller.require_auth();
        assert!(swap.status == SwapStatus::Accepted, "swap not accepted");

        swap.status = SwapStatus::Completed;
        env.storage()
            .persistent()
            .set(&DataKey::Swap(swap_id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);
    }

    /// Cancel a pending swap. Only the seller or buyer may cancel.
    pub fn cancel_swap(env: Env, swap_id: u64, canceller: Address) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ))
            });

        assert!(
            canceller == swap.seller || canceller == swap.buyer,
            "only the seller or buyer can cancel"
        );
        canceller.require_auth();

        assert!(
            swap.status == SwapStatus::Pending,
            "only pending swaps can be cancelled this way"
        );
        swap.status = SwapStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Swap(swap_id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);
    }

    /// Buyer cancels an Accepted swap after expiry.
    pub fn cancel_expired_swap(env: Env, swap_id: u64, caller: Address) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ))
            });

        assert!(
            swap.status == SwapStatus::Accepted,
            "swap not in Accepted state"
        );
        assert!(
            caller == swap.buyer,
            "only the buyer can cancel an expired swap"
        );
        assert!(
            env.ledger().timestamp() > swap.expiry,
            "swap has not expired yet"
        );

        swap.status = SwapStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Swap(swap_id), &swap);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));
    }

    /// Read a swap record. Returns `None` if the swap_id does not exist.
    pub fn get_swap(env: Env, swap_id: u64) -> Option<SwapRecord> {
        env.storage().persistent().get(&DataKey::Swap(swap_id))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ip_registry::{IpRegistry, IpRegistryClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{BytesN, Env};

    /// Registers an IpRegistry contract, commits an IP owned by `owner`,
    /// and returns `(registry_contract_id, ip_id)`.
    fn setup_registry_with_ip(env: &Env, owner: &Address) -> (Address, u64) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let commitment = BytesN::from_array(env, &[0u8; 32]);
        let ip_id = registry.commit_ip(owner, &commitment);
        (registry_id, ip_id)
    }

    fn setup_swap(env: &Env) -> Address {
        env.register(AtomicSwap, ())
    }

    #[test]
    fn get_swap_returns_none_for_nonexistent_id() {
        let env = Env::default();
        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        assert!(client.get_swap(&9999).is_none());
    }

    #[test]
    fn get_swap_returns_some_for_existing_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        let swap_id = client.initiate_swap(&registry_id, &ip_id, &seller, &100_i128, &buyer);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.ip_id, ip_id);
        assert_eq!(swap.price, 100_i128);
        assert_eq!(swap.status, SwapStatus::Pending);
    }

    /// A second `initiate_swap` for the same `ip_id` must be rejected while the first is active.
    #[test]
    fn duplicate_swap_rejected_while_active() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        client.initiate_swap(&registry_id, &ip_id, &seller, &100_i128, &buyer);

        assert!(client
            .try_initiate_swap(&registry_id, &ip_id, &seller, &200_i128, &buyer)
            .is_err());
    }

    /// After a swap is cancelled the IP lock is released and a new swap can be created.
    #[test]
    fn new_swap_allowed_after_cancel() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        let swap_id = client.initiate_swap(&registry_id, &ip_id, &seller, &100_i128, &buyer);
        client.cancel_swap(&swap_id, &seller);

        let new_id = client.initiate_swap(&registry_id, &ip_id, &seller, &150_i128, &buyer);
        assert_ne!(new_id, swap_id);
    }

    /// After a swap completes the IP lock is released and a new swap can be created.
    #[test]
    fn new_swap_allowed_after_complete() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        let swap_id = client.initiate_swap(&registry_id, &ip_id, &seller, &100_i128, &buyer);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &BytesN::from_array(&env, &[0u8; 32]));

        let new_id = client.initiate_swap(&registry_id, &ip_id, &seller, &150_i128, &buyer);
        assert_ne!(new_id, swap_id);
    }

    /// SECURITY: a non-owner must not be able to initiate a swap for an IP they do not own.
    #[test]
    fn non_owner_cannot_initiate_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let real_owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &real_owner);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));

        assert!(
            client
                .try_initiate_swap(&registry_id, &ip_id, &attacker, &999_i128, &buyer)
                .is_err(),
            "expected initiate_swap to fail for non-owner"
        );
    }

    /// SECURITY: initiating a swap for a non-existent ip_id must be rejected.
    /// The cross-call to ip_registry.get_ip panics when the IP does not exist.
    #[test]
    fn invalid_ip_id_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        // Register a registry but do NOT commit any IP — ip_id 9999 does not exist.
        let registry_id = env.register(IpRegistry, ());

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));

        assert!(
            client
                .try_initiate_swap(&registry_id, &9999_u64, &seller, &100_i128, &buyer)
                .is_err(),
            "expected initiate_swap to fail for non-existent ip_id"
        );
    }

    /// SECURITY: a zero price must be rejected to prevent free IP giveaways.
    #[test]
    fn zero_price_rejected() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));

        assert!(
            client
                .try_initiate_swap(&registry_id, &ip_id, &seller, &0_i128, &buyer)
                .is_err(),
            "expected initiate_swap to fail for zero price"
        );
    }

    /// SECURITY: only the designated buyer may accept a swap.
    /// Any other address calling accept_swap must be rejected.
    #[test]
    fn non_buyer_cannot_accept_swap() {
        let env = Env::default();
        // No mock_all_auths — auth checks are fully enforced.

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        // Set up registry and IP with seller auth mocked for setup calls.
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let commitment = BytesN::from_array(&env, &[0u8; 32]);
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &seller,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &registry_id,
                fn_name: "commit_ip",
                args: soroban_sdk::IntoVal::into_val(&(&seller, &commitment), &env),
                sub_invokes: &[],
            },
        }]);
        let ip_id = registry.commit_ip(&seller, &commitment);

        let swap_contract = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &swap_contract);

        // Initiate swap with seller auth.
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &seller,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &swap_contract,
                fn_name: "initiate_swap",
                args: soroban_sdk::IntoVal::into_val(
                    &(&registry_id, &ip_id, &seller, &100_i128, &buyer),
                    &env,
                ),
                sub_invokes: &[],
            },
        }]);
        let swap_id = client.initiate_swap(&registry_id, &ip_id, &seller, &100_i128, &buyer);

        // Attempt accept_swap with NO auth mocked at all.
        // buyer.require_auth() inside accept_swap must reject this call.
        assert!(
            client.try_accept_swap(&swap_id).is_err(),
            "expected accept_swap to fail when buyer auth is not provided"
        );
    }
}

#[cfg(test)]
mod basic_tests;
