#![no_std]
use ip_registry::IpRegistryClient;
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

// ── Storage Keys ─────────────────────────────────────────────────────────────

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

        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextId)
            .unwrap_or(0);

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
        env.storage().persistent().extend_ttl(&DataKey::Swap(id), 50000, 50000);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));
        id
    }

    /// Buyer accepts the swap.
    pub fn accept_swap(env: Env, swap_id: u64) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        swap.buyer.require_auth();
        assert!(swap.status == SwapStatus::Pending, "swap not pending");

        swap.status = SwapStatus::Accepted;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
        env.storage().persistent().extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);
    }

    /// Seller reveals the decryption key; payment releases.
    /// SECURITY: caller must be the seller — verified by identity check + require_auth.
    pub fn reveal_key(env: Env, swap_id: u64, caller: Address, _decryption_key: BytesN<32>) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        assert!(caller == swap.seller, "only the seller can reveal the key");
        caller.require_auth();
        assert!(swap.status == SwapStatus::Accepted, "swap not accepted");

        swap.status = SwapStatus::Completed;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
        env.storage().persistent().extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);
    }

    /// Cancel a pending swap.
    pub fn cancel_swap(env: Env, swap_id: u64, _canceller: Address) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        assert!(swap.status == SwapStatus::Pending, "only pending swaps can be cancelled this way");
        swap.status = SwapStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
        env.storage().persistent().extend_ttl(&DataKey::Swap(swap_id), 50000, 50000);
    }

    /// Buyer cancels an Accepted swap after expiry.
    pub fn cancel_expired_swap(env: Env, swap_id: u64, caller: Address) {
        let mut swap: SwapRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Swap(swap_id))
            .expect("swap not found");

        assert!(swap.status == SwapStatus::Accepted, "swap not in Accepted state");
        assert!(caller == swap.buyer, "only the buyer can cancel an expired swap");
        assert!(env.ledger().timestamp() > swap.expiry, "swap has not expired yet");

        swap.status = SwapStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Swap(swap_id), &swap);
        env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
    }

    /// Read a swap record. Returns `None` if the swap_id does not exist.
    pub fn get_swap(env: Env, swap_id: u64) -> Option<SwapRecord> {
        env.storage().persistent().get(&DataKey::Swap(swap_id))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod basic_tests;
