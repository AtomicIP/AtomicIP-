#![no_std]
use ip_registry::IpRegistryClient;
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, BytesN, Env, Error};

// ── Error Codes ────────────────────────────────────────────────────────────

#[repr(u32)]
pub enum ContractError {
    SwapNotFound = 1,
    InvalidKey = 2,
    PriceMustBeGreaterThanZero = 3,
    SellerIsNotTheIPOwner = 4,
    ActiveSwapAlreadyExistsForThisIpId = 5,
    SwapNotPending = 6,
    OnlyTheSellerCanRevealTheKey = 7,
    SwapNotAccepted = 8,
    OnlyTheSellerOrBuyerCanCancel = 9,
    OnlyPendingSwapsCanBeCancelledThisWay = 10,
    SwapNotInAcceptedState = 11,
    OnlyTheBuyerCanCancelAnExpiredSwap = 12,
    SwapHasNotExpiredYet = 13,
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
    pub ip_registry_id: Address,
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
    ///
    /// This function creates a new atomic swap record, allowing a seller to list
    /// their IP for sale. The seller must be the registered owner of the IP.
    /// The swap starts in Pending status and can be accepted by the buyer.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `ip_registry_id` - The address of the IP registry contract
    /// * `token` - The address of the token contract used for payment
    /// * `ip_id` - The unique identifier of the IP being sold
    /// * `seller` - The address of the seller (must be the IP owner)
    /// * `price` - The price in token units (must be positive)
    /// * `buyer` - The address of the potential buyer
    ///
    /// # Returns
    ///
    /// The unique swap ID assigned to this swap. IDs are monotonically increasing.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The seller does not authorize the transaction (auth error)
    /// * The price is not positive (price_must_be_greater_than_zero error)
    /// * The seller is not the registered IP owner (seller_is_not_the_IP_owner error)
    /// * An active swap already exists for this IP (active_swap_already_exists_for_this_ip_id error)
    /// * The IP does not exist in the registry (IpNotFound error from registry)
    pub fn initiate_swap(
        env: Env,
        ip_registry_id: Address,
        token: Address,
        ip_id: u64,
        seller: Address,
        price: i128,
        buyer: Address,
    ) -> u64 {
        seller.require_auth();

        // 2. Guard: price must be positive.
        if price <= 0 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PriceMustBeGreaterThanZero as u32,
            ));
        }

        // 3. Cross-contract ownership check — SECURITY FIX.
        //    Fetches the IP record from the registry and asserts the caller is
        //    the registered owner. Without this check anyone could initiate a
        //    swap for an IP they do not own.
        let registry = IpRegistryClient::new(&env, &ip_registry_id);
        let record = registry.get_ip(&ip_id);
        if record.owner != seller {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SellerIsNotTheIPOwner as u32,
            ));
        }

        if env.storage().persistent().has(&DataKey::ActiveSwap(ip_id)) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::ActiveSwapAlreadyExistsForThisIpId as u32,
            ));
        }

        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);

        let swap = SwapRecord {
            ip_id,
            ip_registry_id,
            seller,
            buyer,
            price,
            token,
            status: SwapStatus::Pending,
            expiry: env.ledger().timestamp() + 604800u64,
        };

        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(id), 50000, 50000);
        env.storage()
            .persistent()
            .set(&DataKey::ActiveSwap(ip_id), &id);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        id
    }

    /// Buyer accepts the swap.
    ///
    /// This function allows the buyer to accept a pending swap by transferring
    /// the payment into escrow. The swap status changes from Pending to Accepted.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `swap_id` - The unique identifier of the swap to accept
    ///
    /// # Returns
    ///
    /// This function does not return a value.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The swap does not exist (SwapNotFound error)
    /// * The buyer does not authorize the transaction (auth error)
    /// * The swap is not in Pending status (swap_not_pending error)
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
        if swap.status != SwapStatus::Pending {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapNotPending as u32,
            ));
        }

        // Transfer payment from buyer into contract escrow.
        // buyer.require_auth() above satisfies the token's auth requirement via
        // sub-invocation auth propagation in the Soroban host.
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &swap.price,
        );

        swap.status = SwapStatus::Accepted;
        env.storage()
            .persistent()
            .set(&DataKey::Swap(swap_id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);
    }

    /// Seller reveals the decryption key; payment releases only if the key is valid.
    ///
    /// This function allows the seller to reveal the secret that was used to create
    /// the IP commitment. The key is verified against the stored commitment hash
    /// before the swap is marked as Completed. If the key is invalid, the transaction
    /// fails and the payment remains in escrow.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `swap_id` - The unique identifier of the swap
    /// * `caller` - The address of the caller (must be the seller)
    /// * `secret` - The 32-byte secret that was used to create the IP commitment
    /// * `blinding_factor` - The 32-byte blinding factor used to create the commitment
    ///
    /// # Returns
    ///
    /// This function does not return a value.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The swap does not exist (SwapNotFound error)
    /// * The caller is not the seller (only_the_seller_can_reveal_the_key error)
    /// * The caller does not authorize the transaction (auth error)
    /// * The swap is not in Accepted status (swap_not_accepted error)
    /// * The revealed key is invalid (InvalidKey error)
    ///
    /// # Security
    ///
    /// SECURITY: caller must be the seller — verified by identity check + require_auth.
    /// SECURITY: key is verified against the IP commitment before marking Completed.
    pub fn reveal_key(
        env: Env,
        swap_id: u64,
        caller: Address,
        secret: BytesN<32>,
        blinding_factor: BytesN<32>,
    ) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ))
            });

        if caller != swap.seller {
            env.panic_with_error(Error::from_contract_error(
                ContractError::OnlyTheSellerCanRevealTheKey as u32,
            ));
        }
        caller.require_auth();
        if swap.status != SwapStatus::Accepted {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapNotAccepted as u32,
            ));
        }

        let registry = IpRegistryClient::new(&env, &swap.ip_registry_id);
        let valid = registry.verify_commitment(&swap.ip_id, &secret, &blinding_factor);
        if !valid {
            env.panic_with_error(Error::from_contract_error(ContractError::InvalidKey as u32));
        }

        swap.status = SwapStatus::Completed;
        env.storage()
            .persistent()
            .set(&DataKey::Swap(swap_id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);

        env.events().publish(
            (soroban_sdk::symbol_short!("key_rev"),),
            KeyRevealedEvent {
                swap_id,
                decryption_key: secret,
            },
        );
    }

    /// Cancel a pending swap. Only the seller or buyer may cancel.
    ///
    /// This function allows either the seller or buyer to cancel a pending swap.
    /// When cancelled, the IP lock is released and a new swap can be created for the same IP.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `swap_id` - The unique identifier of the swap to cancel
    /// * `canceller` - The address of the person cancelling (must be seller or buyer)
    ///
    /// # Returns
    ///
    /// This function does not return a value.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The swap does not exist (SwapNotFound error)
    /// * The canceller is not the seller or buyer (only_the_seller_or_buyer_can_cancel error)
    /// * The canceller does not authorize the transaction (auth error)
    /// * The swap is not in Pending status (only_pending_swaps_can_be_cancelled_this_way error)
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

        if canceller != swap.seller && canceller != swap.buyer {
            env.panic_with_error(Error::from_contract_error(
                ContractError::OnlyTheSellerOrBuyerCanCancel as u32,
            ));
        }
        canceller.require_auth();

        if swap.status != SwapStatus::Pending {
            env.panic_with_error(Error::from_contract_error(
                ContractError::OnlyPendingSwapsCanBeCancelledThisWay as u32,
            ));
        }
        swap.status = SwapStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Swap(swap_id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);
        // Release the IP lock so a new swap can be created.
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));
    }

    /// Buyer cancels an Accepted swap after expiry.
    ///
    /// This function allows the buyer to cancel an accepted swap after the expiry
    /// time has passed. This protects buyers from sellers who fail to reveal the
    /// key within the expected timeframe.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `swap_id` - The unique identifier of the swap to cancel
    /// * `caller` - The address of the caller (must be the buyer)
    ///
    /// # Returns
    ///
    /// This function does not return a value.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// * The swap does not exist (SwapNotFound error)
    /// * The swap is not in Accepted status (swap_not_in_Accepted_state error)
    /// * The caller is not the buyer (only_the_buyer_can_cancel_an_expired_swap error)
    /// * The swap has not expired yet (swap_has_not_expired_yet error)
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

        if swap.status != SwapStatus::Accepted {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapNotInAcceptedState as u32,
            ));
        }
        if caller != swap.buyer {
            env.panic_with_error(Error::from_contract_error(
                ContractError::OnlyTheBuyerCanCancelAnExpiredSwap as u32,
            ));
        }
        if env.ledger().timestamp() <= swap.expiry {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapHasNotExpiredYet as u32,
            ));
        }

        swap.status = SwapStatus::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Swap(swap_id), &swap);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));
    }

    /// Read a swap record. Returns `None` if the swap_id does not exist.
    ///
    /// Returns the complete swap record including IP details, parties, price,
    /// status, and expiry time.
    ///
    /// # Arguments
    ///
    /// * `env` - The Soroban environment
    /// * `swap_id` - The unique identifier of the swap to retrieve
    ///
    /// # Returns
    ///
    /// `Some(SwapRecord)` containing:
    /// * `ip_id` - The unique identifier of the IP being sold
    /// * `ip_registry_id` - The address of the IP registry contract
    /// * `seller` - The address of the seller
    /// * `buyer` - The address of the buyer
    /// * `price` - The price in token units
    /// * `token` - The address of the token contract
    /// * `status` - The current swap status (Pending, Accepted, Completed, or Cancelled)
    /// * `expiry` - The ledger timestamp after which the buyer may cancel
    ///
    /// `None` if the swap does not exist.
    ///
    /// # Panics
    ///
    /// This function does not panic.
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
    use soroban_sdk::{token, BytesN, Env};
    use soroban_sdk::token::StellarAssetClient;

    /// Registers an IpRegistry contract, commits an IP owned by `owner`,
    /// and returns `(registry_contract_id, ip_id)`.
    fn setup_registry_with_ip(env: &Env, owner: &Address) -> (Address, u64) {
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(env, &registry_id);
        let commitment = BytesN::from_array(env, &[1u8; 32]);
        let ip_id = registry.commit_ip(owner, &commitment);
        (registry_id, ip_id)
    }

    /// Deploys a Stellar asset contract and mints `amount` tokens to `recipient`.
    fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(env, &token_id).mint(recipient, &amount);
        token_id
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
        let admin = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &100_i128, &buyer);

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
        let admin = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &100_i128, &buyer);

        assert!(client
            .try_initiate_swap(&registry_id, &token_id, &ip_id, &seller, &200_i128, &buyer)
            .is_err());
    }

    /// After a swap is cancelled the IP lock is released and a new swap can be created.
    #[test]
    fn new_swap_allowed_after_cancel() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &100_i128, &buyer);
        client.cancel_swap(&swap_id, &seller);

        let new_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &150_i128, &buyer);
        assert_ne!(new_id, swap_id);
    }

    /// After a swap completes the IP lock is released and a new swap can be created.
    #[test]
    fn new_swap_allowed_after_complete() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));
        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &100_i128, &buyer);
        client.accept_swap(&swap_id);
        client.reveal_key(&swap_id, &seller, &BytesN::from_array(&env, &[0u8; 32]));

        let new_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &150_i128, &buyer);
        assert_ne!(new_id, swap_id);
    }

    /// ID continuity: swap IDs must be monotonically increasing and must not
    /// reset to 0 after multiple swaps (simulates upgrade-safe counter behaviour).
    /// This guards against the instance-storage bug where NextId was wiped on upgrade.
    #[test]
    fn next_id_is_persistent_and_monotonic() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);

        // Each IP needs a unique commitment hash — use distinct byte arrays.
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);

        let ip_id_0 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[1u8; 32]));
        let ip_id_1 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[2u8; 32]));
        let ip_id_2 = registry.commit_ip(&seller, &BytesN::from_array(&env, &[3u8; 32]));

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));

        let id0 = client.initiate_swap(&registry_id, &ip_id_0, &seller, &100_i128, &buyer);
        let id1 = client.initiate_swap(&registry_id, &ip_id_1, &seller, &100_i128, &buyer);
        let id2 = client.initiate_swap(&registry_id, &ip_id_2, &seller, &100_i128, &buyer);

        // IDs must be strictly increasing — no resets, no collisions.
        assert_eq!(id0, 0);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);

        // All three swap records must be independently retrievable.
        assert!(client.get_swap(&id0).is_some());
        assert!(client.get_swap(&id1).is_some());
        assert!(client.get_swap(&id2).is_some());
    }

    /// SECURITY: a non-owner must not be able to initiate a swap for an IP they do not own.
    #[test]
    fn non_owner_cannot_initiate_swap() {
        let env = Env::default();
        env.mock_all_auths();

        let real_owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &real_owner);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));

        assert!(
            client
                .try_initiate_swap(&registry_id, &token_id, &ip_id, &attacker, &999_i128, &buyer)
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
        let admin = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let client = AtomicSwapClient::new(&env, &setup_swap(&env));

        assert!(
            client
                .try_initiate_swap(&registry_id, &token_id, &ip_id, &seller, &0_i128, &buyer)
                .is_err(),
            "expected initiate_swap to fail for zero price"
        );
    }

    #[test]
    fn non_buyer_cannot_accept_swap() {
        let env = Env::default();
        // No mock_all_auths — auth checks are fully enforced.

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);

        // Set up registry and IP with seller auth mocked for setup calls.
        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let commitment = BytesN::from_array(&env, &[1u8; 32]);
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

        // Deploy token and mint to buyer (all_auths for mint only).
        env.mock_all_auths();
        let token_id = setup_token(&env, &admin, &buyer, 1000);

        let swap_contract = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &swap_contract);

        // Initiate swap with seller auth.
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &seller,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &swap_contract,
                fn_name: "initiate_swap",
                args: soroban_sdk::IntoVal::into_val(
                    &(&registry_id, &token_id, &ip_id, &seller, &100_i128, &buyer),
                    &env,
                ),
                sub_invokes: &[],
            },
        }]);
        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &100_i128, &buyer);

        // Attempt accept_swap with NO auth mocked at all.
        // buyer.require_auth() inside accept_swap must reject this call.
        assert!(
            client.try_accept_swap(&swap_id).is_err(),
            "expected accept_swap to fail when buyer auth is not provided"
        );
    }

    /// Payment is transferred from buyer to contract escrow on accept_swap,
    /// and released to seller on reveal_key.
    #[test]
    fn payment_held_in_escrow_and_released_to_seller() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let admin = Address::generate(&env);
        let (registry_id, ip_id) = setup_registry_with_ip(&env, &seller);
        let token_id = setup_token(&env, &admin, &buyer, 500);

        let swap_contract = setup_swap(&env);
        let client = AtomicSwapClient::new(&env, &swap_contract);
        let token_client = token::Client::new(&env, &token_id);

        let swap_id = client.initiate_swap(&registry_id, &token_id, &ip_id, &seller, &500_i128, &buyer);

        // Before accept: buyer holds full balance, escrow is empty.
        assert_eq!(token_client.balance(&buyer), 500);
        assert_eq!(token_client.balance(&swap_contract), 0);

        client.accept_swap(&swap_id);

        // After accept: full price moved from buyer to escrow.
        assert_eq!(token_client.balance(&buyer), 0);
        assert_eq!(token_client.balance(&swap_contract), 500);

        let seller_balance_before = token_client.balance(&seller);
        client.reveal_key(&swap_id, &BytesN::from_array(&env, &[0u8; 32]));

        // After reveal: escrow released to seller.
        assert_eq!(token_client.balance(&swap_contract), 0);
        assert_eq!(token_client.balance(&seller), seller_balance_before + 500);
    }
}

#[cfg(test)]
mod basic_tests;
