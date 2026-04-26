#![no_std]
mod registry;
mod swap;
mod upgrade;
mod utils;
mod multi_currency;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token,
    Address, Bytes, BytesN, Env, Error, String, Vec,
};

pub use upgrade::{build_v1_schema, ContractSchema, ErrorEntry, FunctionEntry};

mod validation;
use validation::*;
use multi_currency::{SupportedToken, MultiCurrencyConfig, TokenMetadata};

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
    /// #251: Buyer tried to cancel a pending swap that hasn't expired yet.
    PendingSwapNotExpired = 25,
    /// #252: New expiry must be strictly greater than current expiry.
    NewExpiryNotGreater = 26,
    /// #254: accept_swap called before all required approvals are collected.
    InsufficientApprovals = 27,
    /// #254: Approver has already approved this swap.
    AlreadyApproved = 28,
    /// #311: Referral fee bps exceeds allowed maximum.
    InvalidReferralFeeBps = 35,

    // ── Upgrade-validation errors (29-34) ─────────────────────────────────────
    // NOTE: codes 29-34 are reserved for upgrade validation (see below)

    // ── #314: Arbitration errors (35-37) ──────────────────────────────────────
    /// Arbitrator has already been set for this swap.
    ArbitratorAlreadySet = 35,
    /// Caller is not the designated arbitrator for this swap.
    NotArbitrator = 36,
    /// No arbitrator has been set for this swap.
    NoArbitratorSet = 37,

    // ── #313: Dispute evidence errors (38) ────────────────────────────────────
    /// Caller is not authorized to submit evidence (must be buyer or seller).
    UnauthorizedEvidenceSubmitter = 38,
    /// New schema version must be strictly greater than the current one.
    UpgradeSchemaVersionNotGreater = 29,
    /// A function present in the current schema is missing from the new schema.
    UpgradeMissingFunction = 30,
    /// A function's signature changed between the current and new schema.
    UpgradeFunctionSignatureChanged = 31,
    /// An error entry present in the current schema is missing from the new schema.
    UpgradeMissingErrorCode = 32,
    /// An error's numeric discriminant changed between schemas.
    UpgradeErrorCodeChanged = 33,
    /// A storage key present in the current schema is missing from the new schema.
    UpgradeMissingStorageKey = 34,
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
    /// #253: Maps swap_id → Vec<SwapHistoryEntry> audit trail.
    SwapHistory(u64),
    /// #254: Maps swap_id → Vec<Address> of collected approvals.
    SwapApprovals(u64),
    /// Maps cancellation reason bytes for a swap_id.
    CancelReason(u64),
    /// Multi-currency configuration.
    MultiCurrencyConfig,
    /// List of supported token addresses.
    SupportedTokens,
    /// On-chain interface manifest used by validate_upgrade.
    ContractSchema,
    /// #311: Maps swap_id → referrer Address for referral reward tracking.
    SwapReferrer(u64),
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
    /// #254: Number of approvals required before accept_swap is allowed.
    pub required_approvals: u32,
    /// Ledger timestamp when a dispute was raised. Zero if no dispute.
    pub dispute_timestamp: u64,
    /// #311: Optional referrer address for referral reward on completion.
    pub referrer: Option<Address>,
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
    pub seller_amount: i128,
    pub fee_amount: i128,
}

/// #311: Payload published when a referral reward is paid out.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReferralPaidEvent {
    pub swap_id: u64,
    pub referrer: Address,
    pub referral_amount: i128,
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
    pub dispute_resolution_timeout_seconds: u64,
    /// #311: Referral fee in basis points (0-10000). Deducted from seller proceeds.
    pub referral_fee_bps: u32,
}

// ── #253: Swap History ────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapHistoryEntry {
    pub status: SwapStatus,
    pub timestamp: u64,
}

// ── #252: Expiry Extension Event ──────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapExpiryExtendedEvent {
    pub swap_id: u64,
    pub old_expiry: u64,
    pub new_expiry: u64,
}

// ── #254: Swap Approved Event ─────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapApprovedEvent {
    pub swap_id: u64,
    pub approver: Address,
    pub approvals_count: u32,
}

// ── #314: Arbitration Events ──────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ArbitratorSetEvent {
    pub swap_id: u64,
    pub arbitrator: Address,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ArbitratedEvent {
    pub swap_id: u64,
    pub arbitrator: Address,
    pub refunded: bool,
}

// ── #313: Dispute Evidence Event ──────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DisputeEvidenceSubmittedEvent {
    pub swap_id: u64,
    pub submitter: Address,
    pub evidence_hash: BytesN<32>,
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

        // Seed the on-chain interface manifest so future upgrades can validate
        // backward compatibility against the v1 schema.
        let schema = upgrade::build_v1_schema(&env);
        upgrade::store_schema(&env, &schema);
    }

    /// Seller initiates a patent sale. Returns the swap ID.
    pub fn initiate_swap(
        env: Env,
        token: Address,
        ip_id: u64,
        seller: Address,
        price: i128,
        buyer: Address,
        required_approvals: u32,
        referrer: Option<Address>,
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
            required_approvals,
            dispute_timestamp: 0,
            referrer: referrer.clone(),
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

        // #253: Log initial history entry
        Self::append_history(&env, id, SwapStatus::Pending);

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

        // #254: Ensure all required approvals have been collected.
        if swap.required_approvals > 0 {
            let approvals: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::SwapApprovals(swap_id))
                .unwrap_or(Vec::new(&env));
            if (approvals.len() as u32) < swap.required_approvals {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::InsufficientApprovals as u32,
                ));
            }
        }

        // #312: Apply tiered pricing if tiers are configured.
        let effective_price = if swap.price_tiers.is_empty() {
            swap.price
        } else {
            // Use the last tier whose min_quantity <= 1 (single-unit purchase).
            // For quantity-based calls, callers should use accept_swap_with_quantity.
            swap.price
        };

        // Transfer payment from buyer into contract escrow.
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &effective_price,
        );

        swap.price = effective_price;
        swap.accept_timestamp = env.ledger().timestamp();
        swap.status = SwapStatus::Accepted;

        swap::save_swap(&env, swap_id, &swap);

        // #253: Log history entry
        Self::append_history(&env, swap_id, SwapStatus::Accepted);

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

        // #253: Log history entry
        Self::append_history(&env, swap_id, SwapStatus::Completed);

        // Protocol fee deduction
        let token_client = token::Client::new(&env, &swap.token);
        let config = Self::protocol_config(&env);
        let fee_bps = config.protocol_fee_bps as i128;
        let fee_amount = if fee_bps > 0 && swap.price > 0 {
            (swap.price * fee_bps) / 10000
        } else {
            0
        };

        // #311: Referral fee deduction (from seller proceeds, only if referrer set)
        let referral_amount = if let Some(ref referrer) = swap.referrer {
            let rbps = config.referral_fee_bps as i128;
            if rbps > 0 && swap.price > 0 {
                (swap.price * rbps) / 10000
            } else {
                0
            }
        } else {
            0
        };

        let seller_amount = swap.price - fee_amount - referral_amount;
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
        // #311: Pay referral reward
        if referral_amount > 0 {
            if let Some(ref referrer) = swap.referrer {
                token_client.transfer(
                    &env.current_contract_address(),
                    referrer,
                    &referral_amount,
                );
                env.events().publish(
                    (soroban_sdk::symbol_short!("ref_paid"),),
                    ReferralPaidEvent {
                        swap_id,
                        referrer: referrer.clone(),
                        referral_amount,
                    },
                );
            }
        }
        // Transfer net payment to seller
        token_client.transfer(
            &env.current_contract_address(),
            &swap.seller,
            &seller_amount,
        );

        env.events().publish(
            (soroban_sdk::symbol_short!("key_rev"),),
            KeyRevealedEvent { swap_id, seller_amount, fee_amount },
        );
    }

    /// Buyer raises a dispute on an Accepted swap within the dispute window.
    pub fn raise_dispute(env: Env, swap_id: u64) {
        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();
        require_swap_status(&env, &swap, SwapStatus::Accepted, ContractError::SwapNotAccepted);

        let config = Self::protocol_config(&env);
        let elapsed = env.ledger().timestamp().saturating_sub(swap.accept_timestamp);
        if elapsed >= config.dispute_window_seconds {
            env.panic_with_error(Error::from_contract_error(
                ContractError::DisputeWindowExpired as u32,
            ));
        }

        swap.status = SwapStatus::Disputed;
        swap.dispute_timestamp = env.ledger().timestamp();
        swap::save_swap(&env, swap_id, &swap);

        env.events().publish(
            (soroban_sdk::symbol_short!("disputed"),),
            DisputeRaisedEvent { swap_id },
        );
    }

    /// Admin resolves a disputed swap. refunded=true refunds buyer; false completes to seller.
    pub fn resolve_dispute(env: Env, swap_id: u64, caller: Address, refunded: bool) {
        caller.require_auth();
        require_admin(&env, &caller);

        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::SwapNotDisputed);

        let token_client = token::Client::new(&env, &swap.token);
        if refunded {
            swap.status = SwapStatus::Cancelled;
            swap::save_swap(&env, swap_id, &swap);
            env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
            token_client.transfer(&env.current_contract_address(), &swap.buyer, &swap.price);
            env.storage().persistent().set(
                &DataKey::CancelReason(swap_id),
                &Bytes::from_slice(&env, b"dispute_refund"),
            );
        } else {
            swap.status = SwapStatus::Completed;
            swap::save_swap(&env, swap_id, &swap);
            env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
            let config = Self::protocol_config(&env);
            let fee_amount = if config.protocol_fee_bps > 0 {
                (swap.price * config.protocol_fee_bps as i128) / 10000
            } else {
                0
            };
            let seller_amount = swap.price - fee_amount;
            if fee_amount > 0 {
                token_client.transfer(&env.current_contract_address(), &config.treasury, &fee_amount);
            }
            token_client.transfer(&env.current_contract_address(), &swap.seller, &seller_amount);
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("disp_res"),),
            DisputeResolvedEvent { swap_id, refunded },
        );
    }

    /// Anyone can call after dispute_resolution_timeout_seconds to auto-refund the buyer.
    pub fn auto_resolve_dispute(env: Env, swap_id: u64) {
        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::SwapNotDisputed);

        let config = Self::protocol_config(&env);
        let elapsed = env.ledger().timestamp().saturating_sub(swap.dispute_timestamp);
        if elapsed < config.dispute_resolution_timeout_seconds {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapHasNotExpiredYet as u32,
            ));
        }

        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);
        env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));

        token::Client::new(&env, &swap.token).transfer(
            &env.current_contract_address(),
            &swap.buyer,
            &swap.price,
        );

        env.storage().persistent().set(
            &DataKey::CancelReason(swap_id),
            &Bytes::from_slice(&env, b"dispute_timeout"),
        );

        env.events().publish(
            (soroban_sdk::symbol_short!("disp_res"),),
            DisputeResolvedEvent { swap_id, refunded: true },
        );
    }

    // ── #314: Third-Party Arbitration ─────────────────────────────────────────

    /// Set a neutral third-party arbitrator for a disputed swap.
    /// Only the admin may assign an arbitrator. Can only be set once.
    pub fn set_arbitrator(env: Env, swap_id: u64, caller: Address, arbitrator: Address) {
        caller.require_auth();
        require_admin(&env, &caller);

        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::SwapNotDisputed);

        if swap.arbitrator.is_some() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::ArbitratorAlreadySet as u32,
            ));
        }

        swap.arbitrator = Some(arbitrator.clone());
        swap::save_swap(&env, swap_id, &swap);

        env.storage().persistent().set(&DataKey::Arbitrator(swap_id), &arbitrator);
        env.storage().persistent().extend_ttl(&DataKey::Arbitrator(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_set"),),
            ArbitratorSetEvent { swap_id, arbitrator },
        );
    }

    /// Designated arbitrator resolves a disputed swap.
    /// refunded=true refunds buyer; false completes payment to seller.
    pub fn arbitrate_dispute(env: Env, swap_id: u64, arbitrator: Address, refunded: bool) {
        arbitrator.require_auth();

        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::SwapNotDisputed);

        match &swap.arbitrator {
            None => env.panic_with_error(Error::from_contract_error(
                ContractError::NoArbitratorSet as u32,
            )),
            Some(assigned) => {
                if *assigned != arbitrator {
                    env.panic_with_error(Error::from_contract_error(
                        ContractError::NotArbitrator as u32,
                    ));
                }
            }
        }

        let token_client = token::Client::new(&env, &swap.token);
        if refunded {
            swap.status = SwapStatus::Cancelled;
            swap::save_swap(&env, swap_id, &swap);
            env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
            token_client.transfer(&env.current_contract_address(), &swap.buyer, &swap.price);
            env.storage().persistent().set(
                &DataKey::CancelReason(swap_id),
                &Bytes::from_slice(&env, b"arbitration_refund"),
            );
        } else {
            swap.status = SwapStatus::Completed;
            swap::save_swap(&env, swap_id, &swap);
            env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
            let config = Self::protocol_config(&env);
            let fee_amount = if config.protocol_fee_bps > 0 {
                (swap.price * config.protocol_fee_bps as i128) / 10000
            } else {
                0
            };
            let seller_amount = swap.price - fee_amount;
            if fee_amount > 0 {
                token_client.transfer(&env.current_contract_address(), &config.treasury, &fee_amount);
            }
            token_client.transfer(&env.current_contract_address(), &swap.seller, &seller_amount);
        }

        Self::append_history(&env, swap_id, if refunded { SwapStatus::Cancelled } else { SwapStatus::Completed });

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_res"),),
            ArbitratedEvent { swap_id, arbitrator, refunded },
        );
    }

    // ── #313: Dispute Evidence Submission ─────────────────────────────────────

    /// Submit a hash of off-chain evidence for a disputed swap.
    /// Only the buyer or seller may submit evidence.
    pub fn submit_dispute_evidence(env: Env, swap_id: u64, submitter: Address, evidence_hash: BytesN<32>) {
        submitter.require_auth();

        let swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::SwapNotDisputed);

        if swap.buyer != submitter && swap.seller != submitter {
            env.panic_with_error(Error::from_contract_error(
                ContractError::UnauthorizedEvidenceSubmitter as u32,
            ));
        }

        let mut evidence: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey::DisputeEvidence(swap_id))
            .unwrap_or(Vec::new(&env));

        evidence.push_back(evidence_hash.clone());
        env.storage().persistent().set(&DataKey::DisputeEvidence(swap_id), &evidence);
        env.storage().persistent().extend_ttl(&DataKey::DisputeEvidence(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("evidence"),),
            DisputeEvidenceSubmittedEvent { swap_id, submitter, evidence_hash },
        );
    }

    /// Retrieve all evidence hashes submitted for a disputed swap.
    pub fn get_dispute_evidence(env: Env, swap_id: u64) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::DisputeEvidence(swap_id))
            .unwrap_or(Vec::new(&env))
    }

    // ── #312: Tiered Pricing ──────────────────────────────────────────────────

    /// Accept a swap with a specific quantity, applying tiered pricing.
    /// The effective price is determined by the highest tier whose min_quantity <= quantity.
    /// Falls back to swap.price if no tier matches.
    pub fn accept_swap_with_quantity(env: Env, swap_id: u64, quantity: u32) {
        require_not_paused(&env);

        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::SwapNotPending);

        if swap.required_approvals > 0 {
            let approvals: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::SwapApprovals(swap_id))
                .unwrap_or(Vec::new(&env));
            if (approvals.len() as u32) < swap.required_approvals {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::InsufficientApprovals as u32,
                ));
            }
        }

        // Determine effective price from tiers
        let effective_price = if swap.price_tiers.is_empty() {
            swap.price
        } else {
            let mut best_price = swap.price;
            for i in 0..swap.price_tiers.len() {
                let (min_qty, tier_price) = swap.price_tiers.get(i).unwrap();
                if quantity >= min_qty {
                    best_price = tier_price;
                }
            }
            best_price * quantity as i128
        };

        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &effective_price,
        );

        swap.price = effective_price;
        swap.accept_timestamp = env.ledger().timestamp();
        swap.status = SwapStatus::Accepted;
        swap::save_swap(&env, swap_id, &swap);

        Self::append_history(&env, swap_id, SwapStatus::Accepted);

        env.events().publish(
            (soroban_sdk::symbol_short!("swap_acpt"),),
            SwapAcceptedEvent { swap_id, buyer: swap.buyer },
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

        // #253: Log history entry
        Self::append_history(&env, swap_id, SwapStatus::Cancelled);

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

        // #253: Log history entry
        Self::append_history(&env, swap_id, SwapStatus::Cancelled);

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

    /// Admin-only upgrade with backward-compatibility validation.
    ///
    /// Validates that `new_schema` is a strict superset of the currently stored
    /// interface manifest before swapping the WASM.  The admin must supply the
    /// manifest of the candidate WASM alongside its hash.
    ///
    /// # Upgrade safety requirements
    ///
    /// The following must NOT change across upgrades:
    /// - Exported function names and their full signatures.
    /// - Error code numeric discriminants (names and numbers must be stable).
    /// - Storage key variant names (existing keys must remain readable).
    ///
    /// Additions (new functions, new error codes, new storage keys) are allowed.
    /// The schema version must be strictly greater than the current version.
    pub fn validate_upgrade(
        env: Env,
        new_wasm_hash: BytesN<32>,
        new_schema: ContractSchema,
    ) -> Result<(), ContractError> {
        // Only the admin may trigger an upgrade.
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::UnauthorizedUpgrade as u32,
                ))
            });
        admin.require_auth();

        upgrade::validate_upgrade(&env, new_wasm_hash, new_schema)
    }

    /// Updates the protocol config.
    pub fn admin_set_protocol_config(
        env: Env,
        protocol_fee_bps: u32,
        treasury: Address,
        dispute_window_seconds: u64,
        dispute_resolution_timeout_seconds: u64,
        referral_fee_bps: u32,
    ) {
        if protocol_fee_bps > 10_000 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidFeeBps as u32,
            ));
        }
        if referral_fee_bps > 10_000 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidReferralFeeBps as u32,
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
                dispute_resolution_timeout_seconds,
                referral_fee_bps,
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
                dispute_resolution_timeout_seconds: 2_592_000, // 30 days
                referral_fee_bps: 0,
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

    // ── Multi-Currency Management ──────────────────────────────────────────────

    /// Initialize multi-currency support
    pub fn initialize_multi_currency(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();
        require_admin(&env, &caller);

        let config = MultiCurrencyConfig::initialize(&env);
        env.storage().persistent().set(&DataKey::MultiCurrencyConfig, &config);
        
        // Store supported tokens list
        env.storage().persistent().set(&DataKey::SupportedTokens, &config.enabled_tokens);
        
        Ok(())
    }

    /// Get multi-currency configuration
    pub fn get_multi_currency_config(env: Env) -> Result<MultiCurrencyConfig, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::MultiCurrencyConfig)
            .ok_or(ContractError::SwapNotFound) // Reusing error for "not configured"
    }

    /// Get list of supported tokens
    pub fn get_supported_tokens(env: Env) -> Result<Vec<SupportedToken>, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::SupportedTokens)
            .ok_or(ContractError::SwapNotFound)
    }

    /// Check if a token is supported
    pub fn is_token_supported(env: Env, token: SupportedToken) -> Result<bool, ContractError> {
        let config: MultiCurrencyConfig = env
            .storage()
            .persistent()
            .get(&DataKey::MultiCurrencyConfig)
            .ok_or(ContractError::SwapNotFound)?;
        Ok(config.is_token_supported(&token))
    }

    /// Get token metadata by symbol
    pub fn get_token_metadata(env: Env, symbol: String) -> Result<TokenMetadata, ContractError> {
        let config: MultiCurrencyConfig = env
            .storage()
            .persistent()
            .get(&DataKey::MultiCurrencyConfig)
            .ok_or(ContractError::SwapNotFound)?;
        // Convert soroban String to a fixed-size byte comparison via the module helper
        config.get_token_by_symbol(&env, &symbol).ok_or(ContractError::SwapNotFound)
    }

    /// Add a new supported token (admin only)
    pub fn add_supported_token(
        env: Env,
        caller: Address,
        token: SupportedToken,
        metadata: TokenMetadata,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        require_admin(&env, &caller);

        let mut config: MultiCurrencyConfig = env
            .storage()
            .persistent()
            .get(&DataKey::MultiCurrencyConfig)
            .ok_or(ContractError::SwapNotFound)?;

        if !config.enabled_tokens.contains(token.clone()) {
            let token_addr = metadata.address.clone();
            config.enabled_tokens.push_back(token.clone());
            config.token_metadata.push_back(metadata);

            env.storage().persistent().set(&DataKey::MultiCurrencyConfig, &config);
            env.storage().persistent().set(&DataKey::SupportedTokens, &config.enabled_tokens);

            env.events().publish(
                (symbol_short!("token_add"),),
                multi_currency::TokenAddedEvent {
                    token,
                    address: token_addr,
                },
            );
        }

        Ok(())
    }

    /// Remove a supported token (admin only)
    pub fn remove_supported_token(
        env: Env,
        caller: Address,
        token: SupportedToken,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        require_admin(&env, &caller);

        let config: MultiCurrencyConfig = env
            .storage()
            .persistent()
            .get(&DataKey::MultiCurrencyConfig)
            .ok_or(ContractError::SwapNotFound)?;

        // Cannot remove the default token.
        if config.default_token == token {
            return Err(ContractError::UnauthorizedUpgrade);
        }

        // Removal of non-default tokens is a future enhancement.
        Ok(())
    }

    /// Read a swap record. Returns `None` if the swap_id does not exist.
    pub fn get_swap(env: Env, swap_id: u64) -> Option<SwapRecord> {
        env.storage().persistent().get(&DataKey::Swap(swap_id))
    }

    /// Returns the cancellation reason for a swap, or `None` if not cancelled / reason not set.
    pub fn get_cancellation_reason(env: Env, swap_id: u64) -> Option<Bytes> {
        env.storage().persistent().get(&DataKey::CancelReason(swap_id))
    }

    /// Returns the total number of swaps created.
    pub fn swap_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::NextId).unwrap_or(0)
    }

    // ── #251: Buyer cancel pending swap on timeout ────────────────────────────

    /// Buyer cancels a Pending swap after its expiry. No funds are escrowed at
    /// this stage so no refund transfer is needed.
    pub fn cancel_pending_swap(env: Env, swap_id: u64, caller: Address) {
        let mut swap = require_swap_exists(&env, swap_id);

        require_buyer(&env, &caller, &swap);
        caller.require_auth();
        require_swap_status(
            &env,
            &swap,
            SwapStatus::Pending,
            ContractError::SwapNotPending,
        );

        if env.ledger().timestamp() < swap.expiry {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PendingSwapNotExpired as u32,
            ));
        }

        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));

        Self::append_history(&env, swap_id, SwapStatus::Cancelled);

        env.events().publish(
            (soroban_sdk::symbol_short!("s_cancel"),),
            SwapCancelledEvent {
                swap_id,
                canceller: caller,
            },
        );
    }

    // ── #252: Seller extend swap expiry ──────────────────────────────────────

    /// Seller extends the expiry of a Pending swap to a later timestamp.
    pub fn extend_swap_expiry(env: Env, swap_id: u64, new_expiry: u64) {
        let mut swap = require_swap_exists(&env, swap_id);

        swap.seller.require_auth();
        require_swap_status(
            &env,
            &swap,
            SwapStatus::Pending,
            ContractError::SwapNotPending,
        );

        if new_expiry <= swap.expiry {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NewExpiryNotGreater as u32,
            ));
        }

        let old_expiry = swap.expiry;
        swap.expiry = new_expiry;
        swap::save_swap(&env, swap_id, &swap);

        env.events().publish(
            (soroban_sdk::symbol_short!("exp_ext"),),
            SwapExpiryExtendedEvent {
                swap_id,
                old_expiry,
                new_expiry,
            },
        );
    }

    // ── #253: Swap history / audit trail ─────────────────────────────────────

    /// Returns the full state-transition history for a swap.
    pub fn get_swap_history(env: Env, swap_id: u64) -> Vec<SwapHistoryEntry> {
        env.storage()
            .persistent()
            .get(&DataKey::SwapHistory(swap_id))
            .unwrap_or(Vec::new(&env))
    }

    fn append_history(env: &Env, swap_id: u64, status: SwapStatus) {
        let mut history: Vec<SwapHistoryEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::SwapHistory(swap_id))
            .unwrap_or(Vec::new(env));
        history.push_back(SwapHistoryEntry {
            status,
            timestamp: env.ledger().timestamp(),
        });
        env.storage()
            .persistent()
            .set(&DataKey::SwapHistory(swap_id), &history);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::SwapHistory(swap_id), LEDGER_BUMP, LEDGER_BUMP);
    }

    // ── #254: Multi-sig approval ──────────────────────────────────────────────

    /// Any authorized approver submits their approval for a Pending swap.
    pub fn approve_swap(env: Env, swap_id: u64, approver: Address) {
        approver.require_auth();

        let swap = require_swap_exists(&env, swap_id);
        require_swap_status(
            &env,
            &swap,
            SwapStatus::Pending,
            ContractError::SwapNotPending,
        );

        let mut approvals: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::SwapApprovals(swap_id))
            .unwrap_or(Vec::new(&env));

        // Prevent duplicate approvals
        for i in 0..approvals.len() {
            if approvals.get(i).unwrap() == approver {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::AlreadyApproved as u32,
                ));
            }
        }

        approvals.push_back(approver.clone());
        env.storage()
            .persistent()
            .set(&DataKey::SwapApprovals(swap_id), &approvals);
        env.storage().persistent().extend_ttl(
            &DataKey::SwapApprovals(swap_id),
            LEDGER_BUMP,
            LEDGER_BUMP,
        );

        let approvals_count = approvals.len() as u32;
        env.events().publish(
            (soroban_sdk::symbol_short!("approved"),),
            SwapApprovedEvent {
                swap_id,
                approver,
                approvals_count,
            },
        );
    }

    // ── #309: Batch swap initiation ───────────────────────────────────────────

    /// Seller initiates multiple patent sales in one call. Returns a Vec of swap IDs.
    /// Each ip_ids[i] is paired with prices[i]; all swaps share the same buyer and token.
    pub fn batch_initiate_swap(
        env: Env,
        token: Address,
        ip_ids: Vec<u64>,
        seller: Address,
        prices: Vec<i128>,
        buyer: Address,
        required_approvals: u32,
        referrer: Option<Address>,
    ) -> Vec<u64> {
        require_not_paused(&env);
        seller.require_auth();

        let len = ip_ids.len();
        let mut swap_ids: Vec<u64> = Vec::new(&env);

        for i in 0..len {
            let ip_id = ip_ids.get(i).unwrap();
            let price = prices.get(i).unwrap();

            require_positive_price(&env, price);
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
                required_approvals,
                dispute_timestamp: 0,
                referrer: referrer.clone(),
            };

            env.storage().persistent().set(&DataKey::Swap(id), &swap);
            env.storage().persistent().extend_ttl(&DataKey::Swap(id), LEDGER_BUMP, LEDGER_BUMP);
            env.storage().persistent().set(&DataKey::ActiveSwap(ip_id), &id);
            env.storage().persistent().extend_ttl(&DataKey::ActiveSwap(ip_id), LEDGER_BUMP, LEDGER_BUMP);

            swap::append_swap_for_party(&env, &seller, &buyer, id);

            let mut ip_swap_ids: Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::IpSwaps(ip_id))
                .unwrap_or(Vec::new(&env));
            ip_swap_ids.push_back(id);
            env.storage().persistent().set(&DataKey::IpSwaps(ip_id), &ip_swap_ids);
            env.storage().persistent().extend_ttl(&DataKey::IpSwaps(ip_id), 50000, 50000);

            Self::append_history(&env, id, SwapStatus::Pending);
            env.storage().instance().set(&DataKey::NextId, &(id + 1));

            env.events().publish(
                (soroban_sdk::symbol_short!("swap_init"),),
                SwapInitiatedEvent {
                    swap_id: id,
                    ip_id,
                    seller: seller.clone(),
                    buyer: buyer.clone(),
                    price,
                },
            );

            swap_ids.push_back(id);
        }

        swap_ids
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

#[cfg(test)]
mod prop_tests;

#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod arbitration_tests;
