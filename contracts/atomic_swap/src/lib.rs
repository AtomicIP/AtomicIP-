#![no_std]
mod registry;
mod swap;
mod utils;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, BytesN, Env, Error, Vec,
};

mod validation;
use validation::*;

// ── Error Codes ────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
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
    IpIsRevoked = 14,
    UnauthorizedUpgrade = 15,
    InvalidFeeBps = 16,
    DisputeWindowExpired = 17,
    OnlyBuyerCanDispute = 18,
    SwapNotDisputed = 19,
    OnlyAdminCanResolve = 20,
    ContractPaused = 21,
    AlreadyInitialized = 22,
    Unauthorized = 23,
    NotInitialized = 24,
}

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
pub const LEDGER_BUMP: u32 = 6_307_200;

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Debug, PartialEq)]
pub enum DataKey {
    Swap(u64),
    NextId,
    /// The IpRegistry contract address set once at initialization.
    IpRegistry,
    /// Maps ip_id → swap_id for any swap currently in Pending or Accepted state.
    /// Cleared when a swap reaches Completed or Cancelled.
    ActiveSwap(u64),
    /// Maps seller address → Vec<u64> of all swap IDs they have initiated.
    SellerSwaps(Address),
    /// Maps buyer address → Vec<u64> of all swap IDs they are party to.
    BuyerSwaps(Address),
    Admin,
    ProtocolConfig,
    Paused,
    IpSwaps(u64),
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum SwapStatus {
    Pending,
    Accepted,
    Completed,
    Disputed,
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
    pub accept_timestamp: u64,
}

// ── Events ────────────────────────────────────────────────────────────────────

/// Payload published when a swap is successfully initiated.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapInitiatedEvent {
    pub swap_id: u64,
    pub ip_id: u64,
    pub seller: Address,
    pub buyer: Address,
    pub price: i128,
}

/// Payload published when a swap is successfully accepted.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapAcceptedEvent {
    pub swap_id: u64,
    pub buyer: Address,
}

/// Payload published when a swap is successfully cancelled.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapCancelledEvent {
    pub swap_id: u64,
    pub canceller: Address,
}

/// Payload published when a swap is successfully revealed and the swap completes.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct KeyRevealedEvent {
    pub swap_id: u64,
}

/// Payload published when protocol fee is deducted on swap completion.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolFeeEvent {
    pub swap_id: u64,
    pub fee_amount: i128,
    pub treasury: Address,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DisputeRaisedEvent {
    pub swap_id: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DisputeResolvedEvent {
    pub swap_id: u64,
    pub refunded: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolConfig {
    pub protocol_fee_bps: u32, // 0-10000 (0.00% - 100.00%)
    pub treasury: Address,
    pub dispute_window_seconds: u64,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct AtomicSwap;

#[contractimpl]
impl AtomicSwap {
    /// One-time initialization: store the IpRegistry contract address.
    /// Panics if called more than once.
    pub fn initialize(env: Env, ip_registry: Address) {
        if env.storage().instance().has(&DataKey::IpRegistry) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::AlreadyInitialized as u32,
            ));
        }
        env.storage()
            .instance()
            .set(&DataKey::IpRegistry, &ip_registry);
    }

    /// Seller initiates a patent sale. Returns the swap ID.
    pub fn initiate_swap(
        env: Env,
        token: Address,
        ip_id: u64,
        seller: Address,
        price: i128,
        buyer: Address,
    ) -> u64 {
        // Guard: reject new swaps when the contract is paused.
        require_not_paused(&env);

        seller.require_auth();

        // Initialize admin on first call if not set
        if !env.storage().persistent().has(&DataKey::Admin) {
            env.storage().persistent().set(&DataKey::Admin, &seller);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::Admin, 50000, 50000);
        }

        // Guard: price must be positive.
        require_positive_price(&env, price);

        // Verify seller owns the IP and it's not revoked
        registry::ensure_seller_owns_active_ip(&env, ip_id, &seller);

        require_no_active_swap(&env, ip_id);

        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);

        let swap = SwapRecord {
            ip_id,
            seller: seller.clone(),
            buyer: buyer.clone(),
            price,
            token: token.clone(),
            status: SwapStatus::Pending,
            expiry: env.ledger().timestamp() + 604800u64,
            accept_timestamp: 0,
        };

        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(id), LEDGER_BUMP, LEDGER_BUMP);
        env.storage()
            .persistent()
            .set(&DataKey::ActiveSwap(ip_id), &id);
        env.storage().persistent().extend_ttl(
            &DataKey::ActiveSwap(ip_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        swap::append_swap_for_party(&env, &seller, &buyer, id);

        // Append to ip-swaps index
        let mut ip_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::IpSwaps(ip_id))
            .unwrap_or(Vec::new(&env));
        ip_ids.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::IpSwaps(ip_id), &ip_ids);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::IpSwaps(ip_id), 50000, 50000);

        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        env.events().publish(
            (soroban_sdk::symbol_short!("swap_init"),),
            SwapInitiatedEvent {
                swap_id: id,
                ip_id,
                seller,
                buyer,
                price,
            },
        );

        id
    }

    /// Buyer accepts the swap.
    pub fn accept_swap(env: Env, swap_id: u64) {
        // Guard: reject new acceptances when the contract is paused.
        require_not_paused(&env);

        let mut swap = require_swap_exists(&env, swap_id);

        swap.buyer.require_auth();
        require_swap_status(
            &env,
            &swap,
            SwapStatus::Pending,
            ContractError::SwapNotPending,
        );

        // Transfer payment from buyer into contract escrow.
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &swap.price,
        );

        swap.accept_timestamp = env.ledger().timestamp();
        swap.status = SwapStatus::Accepted;

        swap::save_swap(&env, swap_id, &swap);

        env.events().publish(
            (soroban_sdk::symbol_short!("swap_acpt"),),
            SwapAcceptedEvent {
                swap_id,
                buyer: swap.buyer,
            },
        );
    }

    /// Seller reveals the decryption key; payment releases only if the key is valid.
    pub fn reveal_key(
        env: Env,
        swap_id: u64,
        caller: Address,
        secret: BytesN<32>,
        blinding_factor: BytesN<32>,
    ) {
        let mut swap = require_swap_exists(&env, swap_id);

        require_seller(&env, &caller, &swap);
        caller.require_auth();
        require_swap_status(
            &env,
            &swap,
            SwapStatus::Accepted,
            ContractError::SwapNotAccepted,
        );

        // Verify commitment via IP registry
        let valid = registry::verify_commitment(&env, swap.ip_id, &secret, &blinding_factor);
        if !valid {
            env.panic_with_error(Error::from_contract_error(ContractError::InvalidKey as u32));
        }

        swap.status = SwapStatus::Completed;
        swap::save_swap(&env, swap_id, &swap);

        // Release the IP lock
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));

        // Protocol fee deduction
        let token_client = token::Client::new(&env, &swap.token);
        let config = Self::protocol_config(&env);
        let fee_bps = config.protocol_fee_bps as i128;
        let fee_amount = if fee_bps > 0 && swap.price > 0 {
            (swap.price * fee_bps) / 10000
        } else {
            0
        };
        let seller_amount = swap.price - fee_amount;
        if fee_amount > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &config.treasury,
                &fee_amount,
            );
            env.events().publish(
                (soroban_sdk::symbol_short!("proto_fee"),),
                ProtocolFeeEvent {
                    swap_id,
                    fee_amount,
                    treasury: config.treasury.clone(),
                },
            );
        }
        // Transfer net payment to seller
        token_client.transfer(
            &env.current_contract_address(),
            &swap.seller,
            &seller_amount,
        );

        env.events().publish(
            (soroban_sdk::symbol_short!("key_rev"),),
            KeyRevealedEvent { swap_id },
        );
    }

    /// Cancel a pending swap. Only the seller or buyer may cancel.
    pub fn cancel_swap(env: Env, swap_id: u64, canceller: Address) {
        let mut swap = require_swap_exists(&env, swap_id);

        require_seller_or_buyer(&env, &canceller, &swap);
        canceller.require_auth();

        require_swap_status(
            &env,
            &swap,
            SwapStatus::Pending,
            ContractError::OnlyPendingSwapsCanBeCancelledThisWay,
        );
        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);
        // Release the IP lock so a new swap can be created.
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));

        env.events().publish(
            (soroban_sdk::symbol_short!("swap_cncl"),),
            SwapCancelledEvent { swap_id, canceller },
        );
    }

    /// Buyer cancels an Accepted swap after expiry.
    pub fn cancel_expired_swap(env: Env, swap_id: u64, caller: Address) {
        let mut swap = require_swap_exists(&env, swap_id);

        require_swap_status(
            &env,
            &swap,
            SwapStatus::Accepted,
            ContractError::SwapNotInAcceptedState,
        );
        require_buyer(&env, &caller, &swap);
        require_swap_expired(&env, &swap);

        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));

        // Refund buyer's escrowed payment (Issue #35)
        token::Client::new(&env, &swap.token).transfer(
            &env.current_contract_address(),
            &swap.buyer,
            &swap.price,
        );

        env.events().publish(
            (soroban_sdk::symbol_short!("s_cancel"),),
            SwapCancelledEvent {
                swap_id,
                canceller: caller,
            },
        );
    }

    /// Admin-only contract upgrade.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin_opt = env.storage().persistent().get(&DataKey::Admin);
        if admin_opt.is_none() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::UnauthorizedUpgrade as u32,
            ));
        }
        let admin: Address = admin_opt.unwrap();
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    /// Updates the protocol config.
    pub fn admin_set_protocol_config(
        env: Env,
        protocol_fee_bps: u32,
        treasury: Address,
        dispute_window_seconds: u64,
    ) {
        if protocol_fee_bps > 10_000 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidFeeBps as u32,
            ));
        }

        let caller = env.current_contract_address();
        let admin: Address = if let Some(admin) = env.storage().persistent().get(&DataKey::Admin) {
            admin
        } else {
            caller.require_auth();
            env.storage().persistent().set(&DataKey::Admin, &caller);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::Admin, LEDGER_BUMP, LEDGER_BUMP);
            caller.clone()
        };

        if caller != admin {
            env.panic_with_error(Error::from_contract_error(
                ContractError::UnauthorizedUpgrade as u32,
            ));
        }

        admin.require_auth();
        Self::store_protocol_config(
            &env,
            &ProtocolConfig {
                protocol_fee_bps,
                treasury,
                dispute_window_seconds,
            },
        );
    }

    fn store_protocol_config(env: &Env, config: &ProtocolConfig) {
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolConfig, config);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::ProtocolConfig, LEDGER_BUMP, LEDGER_BUMP);
    }

    fn protocol_config(env: &Env) -> ProtocolConfig {
        env.storage()
            .persistent()
            .get(&DataKey::ProtocolConfig)
            .unwrap_or(ProtocolConfig {
                protocol_fee_bps: 0,
                treasury: env.current_contract_address(),
                dispute_window_seconds: 86400,
            })
    }

    pub fn get_protocol_config(env: Env) -> ProtocolConfig {
        Self::protocol_config(&env)
    }

    /// List all swap IDs initiated by a seller. Returns `None` if the seller has no swaps.
    pub fn get_swaps_by_seller(env: Env, seller: Address) -> Option<Vec<u64>> {
        env.storage()
            .persistent()
            .get(&DataKey::SellerSwaps(seller))
    }

    /// List all swap IDs where the given address is the buyer. Returns `None` if none exist.
    pub fn get_swaps_by_buyer(env: Env, buyer: Address) -> Option<Vec<u64>> {
        env.storage().persistent().get(&DataKey::BuyerSwaps(buyer))
    }

    /// List all swap IDs ever created for a given IP. Returns `None` if none exist.
    pub fn get_swaps_by_ip(env: Env, ip_id: u64) -> Option<Vec<u64>> {
        env.storage().persistent().get(&DataKey::IpSwaps(ip_id))
    }

    /// Set the admin address. Can only be called once (bootstraps the admin).
    pub fn set_admin(env: Env, new_admin: Address) {
        new_admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            // Only the existing admin may rotate the admin key.
            let current: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
            if current != new_admin {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::Unauthorized as u32,
                ));
            }
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    /// Pause the contract. Only the admin may call this.
    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Paused, &true);
    }

    /// Unpause the contract. Only the admin may call this.
    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        require_admin(&env, &caller);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    /// Read a swap record. Returns `None` if the swap_id does not exist.
    pub fn get_swap(env: Env, swap_id: u64) -> Option<SwapRecord> {
        env.storage().persistent().get(&DataKey::Swap(swap_id))
    }

    /// Returns the total number of swaps created.
    pub fn swap_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::NextId).unwrap_or(0)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
