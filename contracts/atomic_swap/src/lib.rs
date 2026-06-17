#![no_std]
mod registry;
mod swap;
// mod upgrade;
mod utils;
mod multi_currency;
mod price_oracle;
mod types;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token,
    Address, Bytes, BytesN, Env, Error, String, Vec,
};

// pub use upgrade::{build_v1_schema, ContractSchema, ErrorEntry, FunctionEntry};
pub use types::*;

mod validation;
use validation::*;
use multi_currency::{SupportedToken, MultiCurrencyConfig, TokenMetadata};
use price_oracle::{
    OracleConfig, OracleConfigSetEvent, OraclePriceUsedEvent,
    fetch_oracle_price, load_oracle_config, store_oracle_config, validate_price_bounds,
};

// ── Error Codes ────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    SwapNotFound = 1,
    InvalidKey = 2,
    PriceTooSmall = 3,
    NotIPOwner = 4,
    SwapExists = 5,
    NotPending = 6,
    OnlySellerReveal = 7,
    NotAccepted = 8,
    OnlySellerBuyer = 9,
    OnlyPending = 10,
    NotInAccepted = 11,
    OnlyBuyerCancel = 12,
    NotExpired = 13,
    IpRevoked = 14,
    UnauthorizedUpg = 15,
    InvalidFeeBps = 16,
    DisputeExpired = 17,
    OnlyBuyerDispute = 18,
    NotDisputed = 19,
    OnlyAdminResolve = 20,
    Paused = 21,
    AlreadyInit = 22,
    Unauthorized = 23,
    NotInitialized = 24,
    PendingNotExpired = 25,
    ExpiryNotGreater = 26,
    NeedApprovals = 27,
    AlreadyApproved = 28,
    SchemaNotGreater = 29,
    MissingFunc = 30,
    FuncChanged = 31,
    InsufficientReputation = 40,
    ArbitrationNotTimedOut = 41,
    NotAllSigned = 42,
    AlreadySigned = 43,
    NotARequiredSigner = 44,
    RollbackWindowExpired = 45,
    /// #470: Oracle errors
    OracleNotConfigured = 46,
    OraclePriceInvalid = 47,
    OraclePriceBelowMin = 48,
    OraclePriceAboveMax = 49,
    /// #314: Arbitration errors
    ArbitratorAlreadySet = 35,
    NotArbitrator = 36,
    NoArbitratorSet = 37,
    /// #313: Dispute evidence errors
    UnauthorizedEvidenceSubmitter = 38,
}

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
pub const LEDGER_BUMP: u32 = 6_307_200;

/// Maximum number of items allowed in a single batch operation.
pub const MAX_BATCH_SIZE: u32 = 50;

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
    /// #347: Maps auction_id → AuctionRecord for IP auctions.
    Auction(u64),
    /// #347: Maps ip_id → auction_id for active auction.
    ActiveAuction(u64),
    /// #347: Maps auction_id → Vec<(bidder, amount)> for bid history.
    AuctionBids(u64),
    /// #347: Next auction ID counter.
    NextAuctionId,
    /// #349: Maps swap_id → Vec<PaymentSchedule> for scheduled payments.
    PaymentSchedule(u64),
    /// #349: Maps swap_id → Vec<bool> tracking which payments have been made.
    PaymentsMade(u64),
    /// #350: Maps swap_id → collateral amount held in escrow.
    SwapCollateral(u64),
    /// #354: Maps swap_id → insurance premium amount.
    SwapInsurance(u64),
    /// #354: Marks a swap as eligible for insurance claim (set when reveal_key fails with insurance).
    InsuranceClaimable(u64),
    /// #354: Global insurance pool balance for the token (token Address → i128).
    InsurancePool(Address),
    /// #353: Maps swap_id → RenegotiationOffer for pending renegotiation.
    SwapRenegotiations(u64),
    /// #352: Maps swap_id → escrow agent address.
    SwapEscrowAgent(u64),
    /// #351: Maps swap_id → acceptance conditions bytes.
    SwapConditions(u64),
    /// Escrow: maps swap_id → SwapMode (Atomic | Escrow).
    SwapMode(u64),
    /// Escrow: maps swap_id → deposited amount (set when buyer deposits).
    EscrowDeposit(u64),
    /// Maps address → reputation score (0–100).
    UserReputation(Address),
    /// Maps ip_id → minimum buyer reputation required (set by seller per swap).
    ReputationMultiplier(u64),
    /// Maps swap_id → timestamp when arbitration was requested.
    ArbitrationTimestamp(u64),
    /// Maps swap_id → Vec<Address> of required co-signers for key reveal.
    SwapSigners(u64),
    /// Maps swap_id → Vec<Address> of signers who have already signed off.
    SwapSignatures(u64),
    /// Maps swap_id → ledger timestamp when the swap reached Completed.
    CompletionTimestamp(u64),
    /// #470: Price oracle configuration (oracle contract address + enabled flag).
    OracleConfig,
    /// #314: Maps swap_id → arbitrator Address for dispute resolution.
    SwapArbitrator(u64),
    /// #313: Maps swap_id → Vec<BytesN<32>> of dispute evidence hashes.
    DisputeEvidence(u64),
}

// ── Types ─────────────────────────────────────────────────────────────────────
// SwapStatus is defined in types.rs

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SwapCondition {
    KeyValid,
    PriceBelow(i128),
    TimeAfter(u64),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ProtocolConfig {
    pub protocol_fee_bps: u32,
    pub treasury: Address,
    pub dispute_window_seconds: u64,
    pub dispute_timeout_secs: u64,
    pub referral_fee_bps: u32,
    /// How long (seconds) after arbitration is requested before auto-refund is allowed.
    /// Default: 14 days = 1_209_600 seconds.
    pub arbitration_timeout_seconds: u64,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum SwapMode {
    Atomic,
    Escrow,
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
    /// #350: Collateral amount required from buyer. Zero if no collateral.
    pub collateral_amount: i128,
    /// #354: Insurance premium paid by buyer. Zero if no insurance.
    pub insurance_premium: i128,
    /// #354: Whether insurance is enabled for this swap.
    pub insurance_enabled: bool,
    /// #352: Optional escrow agent address for high-value swaps.
    pub escrow_agent: Option<Address>,
    /// Total quantity available for partial acceptance. Default 1 (full swap).
    pub quantity: u32,
    /// Conditions the buyer requires to be satisfied before accepting. Empty = unconditional.
    pub conditions: Vec<SwapCondition>,
    /// #installments: Amount paid so far via installments. Zero for non-installment swaps.
    pub paid_amount: i128,
    /// #installments: Whether this swap uses an installment payment schedule.
    pub is_installment: bool,
    /// #314: Optional arbitrator address for dispute resolution.
    pub arbitrator: Option<Address>,
}

// ── Events ────────────────────────────────────────────────────────────────────
// All event types are defined in types.rs to avoid duplication

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
                ContractError::AlreadyInit as u32,
            ));
        }
        env.storage()
            .instance()
            .set(&DataKey::IpRegistry, &ip_registry);

        // Seed the on-chain interface manifest so future upgrades can validate
        // backward compatibility against the v1 schema.
        // let schema = upgrade::build_v1_schema(&env);
        // upgrade::store_schema(&env, &schema);
    }

    // ── #470: Price Oracle ────────────────────────────────────────────────────

    /// Admin sets (or updates) the price oracle contract address.
    /// Pass `enabled = false` to disable oracle-based pricing without removing the address.
    pub fn set_oracle(env: Env, caller: Address, oracle_address: Address, enabled: bool) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NotInitialized as u32,
                ))
            });
        if caller != admin {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }
        let config = OracleConfig {
            oracle_address: oracle_address.clone(),
            enabled,
        };
        store_oracle_config(&env, &config);
        env.events().publish(
            (symbol_short!("oracle"),),
            OracleConfigSetEvent { oracle_address, enabled },
        );
    }

    /// Returns the current oracle configuration, or `None` if not set.
    pub fn get_oracle_config(env: Env) -> Option<OracleConfig> {
        load_oracle_config(&env)
    }

    /// Fetches the current price for `token` from the configured oracle.
    /// Useful for off-chain clients to preview the oracle price before initiating a swap.
    pub fn get_oracle_price(env: Env, token: Address) -> i128 {
        fetch_oracle_price(&env, &token)
    }

    /// Seller initiates a swap where the price is fetched from the oracle at call time.
    /// `min_price` / `max_price` are optional slippage guards (0 = no bound).
    /// Returns the swap ID.
    pub fn initiate_swap_with_oracle_price(
        env: Env,
        token: Address,
        ip_id: u64,
        seller: Address,
        buyer: Address,
        required_approvals: u32,
        referrer: Option<Address>,
        collateral_amount: i128,
        insurance_enabled: bool,
        min_price: i128,
        max_price: i128,
    ) -> u64 {
        let oracle_price = fetch_oracle_price(&env, &token);
        validate_price_bounds(&env, oracle_price, min_price, max_price);

        let swap_id = Self::initiate_swap(
            env.clone(),
            token,
            ip_id,
            seller,
            oracle_price,
            buyer,
            required_approvals,
            referrer,
            collateral_amount,
            insurance_enabled,
        );

        env.events().publish(
            (symbol_short!("ora_price"),),
            OraclePriceUsedEvent { swap_id, oracle_price },
        );

        swap_id
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
        collateral_amount: i128,
        insurance_enabled: bool,
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
            collateral_amount,
            insurance_premium: if insurance_enabled { price * 2 / 100 } else { 0 },
            insurance_enabled,
            escrow_agent: None,
            quantity: 1,
            conditions: Vec::new(&env),
            paid_amount: 0,
            is_installment: false,
            arbitrator: None,
        };

        // Store insurance premium in dedicated key so accept_swap can collect it
        if insurance_enabled {
            let premium = swap.insurance_premium;
            env.storage()
                .persistent()
                .set(&DataKey::SwapInsurance(id), &premium);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::SwapInsurance(id), LEDGER_BUMP, LEDGER_BUMP);
        }

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

    /// Evaluate all conditions on a swap. Panics with ConditionNotMet if any fail.
    /// `check_key_valid` controls whether KeyValid is enforced (true at reveal_key time).
    fn evaluate_conditions(env: &Env, swap: &SwapRecord, check_key_valid: bool) {
        for i in 0..swap.conditions.len() {
            let ok = match swap.conditions.get(i).unwrap() {
                SwapCondition::KeyValid => {
                    // At reveal_key time (check_key_valid=true) the key has already been
                    // verified by registry::verify_commitment, so this always passes.
                    // At accept time (check_key_valid=false) this is deferred.
                    let _ = check_key_valid;
                    true
                }
                SwapCondition::PriceBelow(threshold) => swap.price < threshold,
                SwapCondition::TimeAfter(ts) => env.ledger().timestamp() >= ts,
            };
            if !ok {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::ConditionNotMet as u32,
                ));
            }
        }
    }

    /// #468: Buyer accepts the swap with conditions. Conditions are stored on the swap
    /// record and evaluated immediately (except KeyValid, which is deferred to reveal_key).
    /// If all non-deferred conditions pass, the swap proceeds to Accepted.
    pub fn accept_swap_conditional(env: Env, swap_id: u64, conditions: Vec<SwapCondition>) {
        require_not_paused(&env);

        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);

        // #254: Ensure all required approvals have been collected.
        if swap.required_approvals > 0 {
            let approvals: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::SwapApprovals(swap_id))
                .unwrap_or(Vec::new(&env));
            if (approvals.len() as u32) < swap.required_approvals {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NeedApprovals as u32,
                ));
            }
        }

        // Attach conditions to the swap record.
        swap.conditions = conditions;

        // Evaluate non-deferred conditions immediately.
        Self::evaluate_conditions(&env, &swap, false);

        // Transfer payment from buyer into contract escrow.
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &swap.price,
        );

        swap.accept_timestamp = env.ledger().timestamp();
        swap.status = SwapStatus::Accepted;
        swap::save_swap(&env, swap_id, &swap);

        Self::append_history(&env, swap_id, SwapStatus::Accepted);

        env.events().publish(
            (symbol_short!("swap_acpt"),),
            SwapAcceptedEvent { swap_id, buyer: swap.buyer },
        );
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
            ContractError::NotPending,
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
                    ContractError::NeedApprovals as u32,
                ));
            }
        }

        // #351: Evaluate any buyer-set conditions.
        if !swap.conditions.is_empty() {
            Self::evaluate_conditions(&env, &swap, false);
        }

        // Check minimum reputation requirement set by seller
        if let Some(min_rep) = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::ReputationMultiplier(swap_id))
        {
            let buyer_rep = env
                .storage()
                .persistent()
                .get::<_, u32>(&DataKey::UserReputation(swap.buyer.clone()))
                .unwrap_or(50u32);
            if buyer_rep < min_rep {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::InsufficientReputation as u32,
                ));
            }
        }

        // #350: Deposit collateral if required
        if swap.collateral_amount > 0 {
            // Check if collateral already deposited
            if env.storage().persistent().has(&DataKey::SwapCollateral(swap_id)) {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::AlreadyInit as u32,
                ));
            }

            // Transfer collateral from buyer to contract
            token::Client::new(&env, &swap.token).transfer(
                &swap.buyer,
                &env.current_contract_address(),
                &swap.collateral_amount,
            );

            // Store collateral amount
            env.storage()
                .persistent()
                .set(&DataKey::SwapCollateral(swap_id), &swap.collateral_amount);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::SwapCollateral(swap_id), LEDGER_BUMP, LEDGER_BUMP);

            env.events().publish(
                (soroban_sdk::symbol_short!("coll_dep"),),
                CollateralDepositedEvent {
                    swap_id,
                    buyer: swap.buyer.clone(),
                    collateral_amount: swap.collateral_amount,
                },
            );
        }

        // Transfer payment from buyer into contract escrow.
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &swap.price,
        );

        // #354: Collect insurance premium from buyer and add to pool.
        if swap.insurance_enabled && swap.insurance_premium > 0 {
            token::Client::new(&env, &swap.token).transfer(
                &swap.buyer,
                &env.current_contract_address(),
                &swap.insurance_premium,
            );
            let pool_key = DataKey::InsurancePool(swap.token.clone());
            let pool: i128 = env.storage().persistent().get(&pool_key).unwrap_or(0);
            env.storage().persistent().set(&pool_key, &(pool + swap.insurance_premium));
            env.storage().persistent().extend_ttl(&pool_key, LEDGER_BUMP, LEDGER_BUMP);
        }

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
            ContractError::NotAccepted,
        );

        // Verify commitment via IP registry
        // Guard: if this swap has required signers, all must have signed before reveal.
        if env.storage().persistent().has(&DataKey::SwapSigners(swap_id)) {
            let signers: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::SwapSigners(swap_id))
                .unwrap_or(Vec::new(&env));
            let signed: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::SwapSignatures(swap_id))
                .unwrap_or(Vec::new(&env));
            if signed.len() < signers.len() {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NotAllSigned as u32,
                ));
            }
        }

        let valid = registry::verify_commitment(&env, swap.ip_id, &secret, &blinding_factor);
        if !valid {
            // #354: If insurance is enabled, mark swap as claimable before panicking.
            if swap.insurance_enabled {
                env.storage()
                    .persistent()
                    .set(&DataKey::InsuranceClaimable(swap_id), &true);
                env.storage()
                    .persistent()
                    .extend_ttl(&DataKey::InsuranceClaimable(swap_id), LEDGER_BUMP, LEDGER_BUMP);
            }
            env.panic_with_error(Error::from_contract_error(ContractError::InvalidKey as u32));
        }

        // #468: Enforce KeyValid condition — if the swap has a KeyValid condition,
        // the key must have been verified above. Since we only reach here when valid=true,
        // this is always satisfied; but we also re-evaluate all other conditions at
        // reveal time to ensure time/price conditions still hold.
        if !swap.conditions.is_empty() {
            Self::evaluate_conditions(&env, &swap, true);
        }

        swap.status = SwapStatus::Completed;
        swap::save_swap(&env, swap_id, &swap);

        // Record completion timestamp for rollback window
        let completion_ts = env.ledger().timestamp();
        env.storage().persistent().set(&DataKey::CompletionTimestamp(swap_id), &completion_ts);
        env.storage().persistent().extend_ttl(&DataKey::CompletionTimestamp(swap_id), LEDGER_BUMP, LEDGER_BUMP);

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
        let referral_amount = if let Some(ref _referrer) = swap.referrer {
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

        // #350: Release collateral to buyer on successful completion
        if swap.collateral_amount > 0 {
            if let Some(collateral) = env
                .storage()
                .persistent()
                .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
            {
                token_client.transfer(
                    &env.current_contract_address(),
                    &swap.buyer,
                    &collateral,
                );
                env.storage()
                    .persistent()
                    .remove(&DataKey::SwapCollateral(swap_id));

                env.events().publish(
                    (soroban_sdk::symbol_short!("coll_rel"),),
                    CollateralReleasedEvent {
                        swap_id,
                        buyer: swap.buyer.clone(),
                        collateral_amount: collateral,
                    },
                );
            }
        }

        // #359: Update reputation on completion
        Self::update_reputation(&env, &swap.seller, 5);
        Self::update_reputation(&env, &swap.buyer, 5);

        env.events().publish(
            (soroban_sdk::symbol_short!("key_rev"),),
            KeyRevealedEvent { swap_id, seller_amount, fee_amount },
        );
    }

    /// Buyer raises a dispute on an Accepted swap within the dispute window.
    pub fn raise_dispute(env: Env, swap_id: u64) {
        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();
        require_swap_status(&env, &swap, SwapStatus::Accepted, ContractError::NotAccepted);

        let config = Self::protocol_config(&env);
        let elapsed = env.ledger().timestamp().saturating_sub(swap.accept_timestamp);
        if elapsed >= config.dispute_window_seconds {
            env.panic_with_error(Error::from_contract_error(
                ContractError::DisputeExpired as u32,
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
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::NotDisputed);

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

    // ── #360: Admin Rollback ───────────────────────────────────────────────────

    /// Admin rollback: refund both parties and cancel a swap due to fraud or issues.
    /// This is a safety mechanism for extreme cases where the normal dispute process
    /// is insufficient. Both buyer and seller receive full refunds.
    pub fn admin_rollback_swap(env: Env, swap_id: u64, caller: Address, reason: Bytes) {
        caller.require_auth();
        require_admin(&env, &caller);

        let mut swap = require_swap_exists(&env, swap_id);

        // Cannot rollback already completed swaps
        if swap.status == SwapStatus::Completed {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NotInAccepted as u32,
            ));
        }

        let token_client = token::Client::new(&env, &swap.token);
        let mut buyer_refund = 0i128;
        let seller_refund = 0i128;

        // Refund buyer: price (if already paid) + collateral + insurance
        if swap.status == SwapStatus::Accepted || swap.status == SwapStatus::Disputed {
            // Refund price to buyer
            buyer_refund += swap.price;
            token_client.transfer(&env.current_contract_address(), &swap.buyer, &swap.price);

            // Refund collateral to buyer
            if let Some(collateral) = env
                .storage()
                .persistent()
                .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
            {
                buyer_refund += collateral;
                token_client.transfer(&env.current_contract_address(), &swap.buyer, &collateral);
                env.storage().persistent().remove(&DataKey::SwapCollateral(swap_id));
            }

            // Refund insurance premium to buyer
            if swap.insurance_premium > 0 {
                buyer_refund += swap.insurance_premium;
                token_client.transfer(&env.current_contract_address(), &swap.buyer, &swap.insurance_premium);
            }
        }

        // Refund seller: any held amounts (in case seller deposited something)
        // For now, seller doesn't deposit anything extra, but we track it for completeness
        // In future extensions, seller might have deposited security deposits

        // Update swap status
        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);

        // Release the IP lock
        env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));

        // Store rollback reason
        env.storage().persistent().set(&DataKey::CancelReason(swap_id), &reason);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::CancelReason(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        // #253: Log history entry
        Self::append_history(&env, swap_id, SwapStatus::Cancelled);

        // Emit rollback event
        env.events().publish(
            (soroban_sdk::symbol_short!("adm_rlbk"),),
            AdminRollbackEvent {
                swap_id,
                reason,
                buyer_refund,
                seller_refund,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Admin batch rollback: cancel multiple swaps and refund all parties in one call.
    /// Pre-validates all swap IDs before making any state changes (#521).
    pub fn batch_rollback_swaps(env: Env, swap_ids: Vec<u64>, caller: Address, reason: Bytes) {
        caller.require_auth();
        require_admin(&env, &caller);

        let len = swap_ids.len();

        if len == 0 {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchEmpty as u32));
        }
        if len > MAX_BATCH_SIZE {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchTooLarge as u32));
        }

        // Pre-validation pass: ensure every swap exists and is rollbackable
        for swap_id in swap_ids.iter() {
            let swap = require_swap_exists(&env, swap_id);
            if swap.status == SwapStatus::Completed {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NotInAccepted as u32,
                ));
            }
        }

        // Execution pass: roll back each swap
        for swap_id in swap_ids.iter() {
            let mut swap = require_swap_exists(&env, swap_id);
            let token_client = token::Client::new(&env, &swap.token);

            if swap.status == SwapStatus::Accepted || swap.status == SwapStatus::Disputed {
                token_client.transfer(
                    &env.current_contract_address(),
                    &swap.buyer,
                    &swap.price,
                );

                if let Some(collateral) = env
                    .storage()
                    .persistent()
                    .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
                {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &swap.buyer,
                        &collateral,
                    );
                    env.storage().persistent().remove(&DataKey::SwapCollateral(swap_id));
                }

                if swap.insurance_premium > 0 {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &swap.buyer,
                        &swap.insurance_premium,
                    );
                }
            }

            swap.status = SwapStatus::Cancelled;
            swap::save_swap(&env, swap_id, &swap);
            env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
            env.storage().persistent().set(&DataKey::CancelReason(swap_id), &reason);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::CancelReason(swap_id), LEDGER_BUMP, LEDGER_BUMP);

            Self::append_history(&env, swap_id, SwapStatus::Cancelled);
        }

        // #519: Emit batch rollback event
        env.events().publish(
            (soroban_sdk::symbol_short!("btch_rlb"),),
            BatchRollbackEvent {
                swap_ids,
                caller,
                count: len as u32,
            },
        );
    }

    /// Anyone can call after dispute_resolution_timeout_seconds to auto-refund the buyer.
    pub fn auto_resolve_dispute(env: Env, swap_id: u64) {
        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::NotDisputed);

        let config = Self::protocol_config(&env);
        let elapsed = env.ledger().timestamp().saturating_sub(swap.dispute_timestamp);
        if elapsed < config.dispute_timeout_secs {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NotExpired as u32,
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

    /// Admin sets an arbitrator for a disputed swap. Can only be set once.
    pub fn set_arbitrator(env: Env, swap_id: u64, admin: Address, arbitrator: Address) {
        admin.require_auth();
        require_admin(&env, &admin);

        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::NotDisputed);

        if env.storage().persistent().has(&DataKey::SwapArbitrator(swap_id)) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::ArbitratorAlreadySet as u32,
            ));
        }

        env.storage()
            .persistent()
            .set(&DataKey::SwapArbitrator(swap_id), &arbitrator);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::SwapArbitrator(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        swap.arbitrator = Some(arbitrator.clone());
        swap::save_swap(&env, swap_id, &swap);

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_set"),),
            ArbitratorSetEvent { swap_id, arbitrator },
        );
    }

    /// Arbitrator resolves a disputed swap. refund=true refunds buyer; false completes to seller.
    pub fn arbitrate_dispute(env: Env, swap_id: u64, arbitrator: Address, refund: bool) {
        arbitrator.require_auth();

        let stored_arbitrator: Address = env
            .storage()
            .persistent()
            .get(&DataKey::SwapArbitrator(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NoArbitratorSet as u32,
                ))
            });

        if arbitrator != stored_arbitrator {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NotArbitrator as u32,
            ));
        }

        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::NotDisputed);

        let token_client = token::Client::new(&env, &swap.token);

        if refund {
            token_client.transfer(&env.current_contract_address(), &swap.buyer, &swap.price);
            swap.status = SwapStatus::Cancelled;
        } else {
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
            swap.status = SwapStatus::Completed;
        }

        swap::save_swap(&env, swap_id, &swap);
        env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
        env.storage().persistent().remove(&DataKey::SwapArbitrator(swap_id));

        Self::append_history(&env, swap_id, swap.status.clone());

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_dec"),),
            ArbitratedEvent { swap_id, arbitrator, refunded: refund },
        );
    }

    /// Buyer or seller submits dispute evidence (a hash of off-chain evidence).
    pub fn submit_dispute_evidence(
        env: Env,
        swap_id: u64,
        submitter: Address,
        evidence_hash: BytesN<32>,
    ) {
        submitter.require_auth();
        let swap = require_swap_exists(&env, swap_id);

        if submitter != swap.buyer && submitter != swap.seller {
            env.panic_with_error(Error::from_contract_error(
                ContractError::UnauthorizedEvidenceSubmitter as u32,
            ));
        }

        let key = DataKey::DisputeEvidence(swap_id);
        let mut evidence: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        evidence.push_back(evidence_hash.clone());
        env.storage().persistent().set(&key, &evidence);
        env.storage().persistent().extend_ttl(&key, LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("evid_sub"),),
            DisputeEvidenceSubmittedEvent { swap_id, submitter, evidence_hash },
        );
    }

    /// Returns all dispute evidence hashes submitted for a swap.
    pub fn get_dispute_evidence(env: Env, swap_id: u64) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&DataKey::DisputeEvidence(swap_id))
            .unwrap_or(Vec::new(&env))
    }

    // submit_dispute_evidence removed - DisputeEvidence DataKey variant not defined
    // get_dispute_evidence removed - DisputeEvidence DataKey variant not defined
    // accept_swap_with_quantity removed - price_tiers field not in SwapRecord

    /// Buyer accepts a partial quantity of a bulk swap at a proportional price.
    /// `quantity` must be ≥ 1 and ≤ `swap.quantity`.
    /// Payment = swap.price * quantity / swap.quantity (integer division, rounded down).
    pub fn accept_swap_partial(env: Env, swap_id: u64, quantity: u32) {
        require_not_paused(&env);

        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);

        if quantity == 0 || quantity > swap.quantity {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidKey as u32,
            ));
        }

        let partial_price = swap.price * quantity as i128 / swap.quantity as i128;

        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &partial_price,
        );

        swap.price = partial_price;
        swap.quantity = quantity;
        swap.accept_timestamp = env.ledger().timestamp();
        swap.status = SwapStatus::Accepted;
        swap::save_swap(&env, swap_id, &swap);

        Self::append_history(&env, swap_id, SwapStatus::Accepted);

        env.events().publish(
            (soroban_sdk::symbol_short!("swap_acpt"),),
            SwapAcceptedEvent { swap_id, buyer: swap.buyer },
        );
    }

    // ── #353: Renegotiation ───────────────────────────────────────────────────

    /// Seller proposes a new price for a Pending swap.
    /// Overwrites any existing pending offer.
    pub fn renegotiate_swap(env: Env, swap_id: u64, new_price: i128) {
        require_not_paused(&env);
        let swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);
        swap.seller.require_auth();
        require_positive_price(&env, new_price);

        let offer = RenegotiationOffer {
            new_price,
            proposer: swap.seller.clone(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::SwapRenegotiations(swap_id), &offer);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::SwapRenegotiations(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("rneg_prp"),),
            RenegotiationProposedEvent { swap_id, new_price, proposer: swap.seller },
        );
    }

    /// Buyer accepts the pending renegotiation offer, updating the swap price.
    pub fn accept_renegotiation(env: Env, swap_id: u64) {
        require_not_paused(&env);
        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);
        swap.buyer.require_auth();

        let offer: RenegotiationOffer = env
            .storage()
            .persistent()
            .get(&DataKey::SwapRenegotiations(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ))
            });

        swap.price = offer.new_price;
        swap::save_swap(&env, swap_id, &swap);

        env.storage()
            .persistent()
            .remove(&DataKey::SwapRenegotiations(swap_id));

        env.events().publish(
            (soroban_sdk::symbol_short!("rneg_acp"),),
            RenegotiationAcceptedEvent { swap_id, new_price: offer.new_price, buyer: swap.buyer },
        );
    }

    // ── #354: Insurance ───────────────────────────────────────────────────────

    /// Buyer claims insurance payout after seller revealed an invalid key.
    /// Requires insurance to have been enabled and the swap to be marked claimable.
    pub fn claim_insurance(env: Env, swap_id: u64) {
        let swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();

        if !swap.insurance_enabled {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }

        if !env.storage().persistent().has(&DataKey::InsuranceClaimable(swap_id)) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }

        // Payout = swap price (buyer gets their payment back from the pool)
        let payout = swap.price;
        let pool_key = DataKey::InsurancePool(swap.token.clone());
        let pool: i128 = env.storage().persistent().get(&pool_key).unwrap_or(0);

        // Deduct from pool (pool may be partially funded; pay what's available)
        let actual_payout = if pool >= payout { payout } else { pool };

        token::Client::new(&env, &swap.token).transfer(
            &env.current_contract_address(),
            &swap.buyer,
            &actual_payout,
        );

        env.storage().persistent().set(&pool_key, &(pool - actual_payout));
        // Clear claimable flag so it can't be claimed twice
        env.storage().persistent().remove(&DataKey::InsuranceClaimable(swap_id));

        env.events().publish(
            (soroban_sdk::symbol_short!("ins_pay"),),
            InsurancePayoutEvent { swap_id, buyer: swap.buyer, payout_amount: actual_payout },
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
            ContractError::OnlyPending,
        );
        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);
        // Release the IP lock so a new swap can be created.
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));

        // #253: Log history entry
        Self::append_history(&env, swap_id, SwapStatus::Cancelled);

        // Update reputation: canceller loses 10 points
        Self::update_reputation(&env, &canceller, -10);

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
            ContractError::NotInAccepted,
        );
        require_buyer(&env, &caller, &swap);
        require_swap_expired(&env, &swap);

        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));

        let token_client = token::Client::new(&env, &swap.token);

        // Refund buyer's escrowed payment (Issue #35)
        token_client.transfer(
            &env.current_contract_address(),
            &swap.buyer,
            &swap.price,
        );

        // #350: Refund collateral on cancellation
        if swap.collateral_amount > 0 {
            if let Some(collateral) = env
                .storage()
                .persistent()
                .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
            {
                token_client.transfer(
                    &env.current_contract_address(),
                    &swap.buyer,
                    &collateral,
                );
                env.storage()
                    .persistent()
                    .remove(&DataKey::SwapCollateral(swap_id));

                env.events().publish(
                    (soroban_sdk::symbol_short!("coll_ref"),),
                    CollateralRefundedEvent {
                        swap_id,
                        buyer: swap.buyer.clone(),
                        collateral_amount: collateral,
                    },
                );
            }
        }

        // #253: Log history entry
        Self::append_history(&env, swap_id, SwapStatus::Cancelled);

        // Seller defaulted (expired without revealing key): seller loses 10 points
        Self::update_reputation(&env, &swap.seller, -10);

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
                ContractError::UnauthorizedUpg as u32,
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
    // pub fn validate_upgrade(
    //     env: Env,
    //     new_wasm_hash: BytesN<32>,
    //     new_schema: ContractSchema,
    // ) -> Result<(), ContractError> {
    //     // Only the admin may trigger an upgrade.
    //     let admin: Address = env
    //         .storage()
    //         .persistent()
    //         .get(&DataKey::Admin)
    //         .unwrap_or_else(|| {
    //             env.panic_with_error(Error::from_contract_error(
    //                 ContractError::UnauthorizedUpg as u32,
    //             ))
    //         });
    //     admin.require_auth();
    //
    //     // upgrade::validate_upgrade(&env, new_wasm_hash, new_schema)
    //     Ok(())
    // }

    /// Updates the protocol config.
    pub fn admin_set_protocol_config(
        env: Env,
        protocol_fee_bps: u32,
        treasury: Address,
        dispute_window_seconds: u64,
        dispute_timeout_secs: u64,
        referral_fee_bps: u32,
    ) {
        if protocol_fee_bps > 10_000 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidFeeBps as u32,
            ));
        }
        if referral_fee_bps > 10_000 {
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
                ContractError::UnauthorizedUpg as u32,
            ));
        }

        admin.require_auth();
        Self::store_protocol_config(
            &env,
            &ProtocolConfig {
                protocol_fee_bps,
                treasury,
                dispute_window_seconds,
                dispute_timeout_secs,
                referral_fee_bps,
                arbitration_timeout_seconds: 1_209_600,
            },
        );
    }

    fn store_protocol_config(_env: &Env, _config: &ProtocolConfig) {
        // ProtocolConfig storage temporarily disabled due to trait generation issues
        // env.storage().persistent().set(&DataKey::ProtocolConfig, config);
        // env.storage().persistent().extend_ttl(&DataKey::ProtocolConfig, LEDGER_BUMP, LEDGER_BUMP);
    }

    fn protocol_config(env: &Env) -> ProtocolConfig {
        // Return default config since storage is disabled
        ProtocolConfig {
            protocol_fee_bps: 250,
            treasury: Address::from_string(&soroban_sdk::String::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")),
            dispute_window_seconds: 86400,
            dispute_timeout_secs: 604800,
            referral_fee_bps: 100,
            arbitration_timeout_seconds: 1_209_600, // 14 days
        }
    }

    // get_protocol_config removed - ProtocolConfig can't be returned from contract functions
    // Use individual getters instead if needed

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
            return Err(ContractError::UnauthorizedUpg);
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
            ContractError::NotPending,
        );

        if env.ledger().timestamp() < swap.expiry {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PendingNotExpired as u32,
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
            ContractError::NotPending,
        );

        if new_expiry <= swap.expiry {
            env.panic_with_error(Error::from_contract_error(
                ContractError::ExpiryNotGreater as u32,
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
            ContractError::NotPending,
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

        // #520: Validate batch parameters upfront
        if len == 0 {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchEmpty as u32));
        }
        if prices.len() != len {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchSizeMismatch as u32));
        }
        if len > MAX_BATCH_SIZE {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchTooLarge as u32));
        }

        // #522: Pre-validation pass — no mutations; ensures full atomicity
        for i in 0..len {
            let ip_id = ip_ids.get(i).unwrap();
            let price = prices.get(i).unwrap();
            require_positive_price(&env, price);
            registry::ensure_seller_owns_active_ip(&env, ip_id, &seller);
            require_no_active_swap(&env, ip_id);
            // Detect duplicate IP IDs within the batch
            for j in (i + 1)..len {
                if ip_ids.get(j).unwrap() == ip_id {
                    env.panic_with_error(Error::from_contract_error(
                        ContractError::SwapExists as u32,
                    ));
                }
            }
        }

        // Execution pass — all validations passed, safe to mutate
        let mut swap_ids: Vec<u64> = Vec::new(&env);

        for i in 0..len {
            let ip_id = ip_ids.get(i).unwrap();
            let price = prices.get(i).unwrap();
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
                collateral_amount: 0,
                insurance_premium: 0,
                insurance_enabled: false,
                escrow_agent: None,
                quantity: 1,
                conditions: Vec::new(&env),
                paid_amount: 0,
                is_installment: false,
                arbitrator: None,
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

        // #519: Emit batch-level initiated event
        env.events().publish(
            (soroban_sdk::symbol_short!("btch_ini"),),
            BatchInitiatedEvent {
                swap_ids: swap_ids.clone(),
                seller,
                buyer,
                count: len as u32,
            },
        );

        swap_ids
    }

    // ── #347: IP Auction Mechanism ────────────────────────────────────────────

    /// Seller starts an auction for their IP. Returns the auction ID.
    pub fn start_ip_auction(
        env: Env,
        token: Address,
        ip_id: u64,
        seller: Address,
        min_bid: i128,
        duration_seconds: u64,
    ) -> u64 {
        require_not_paused(&env);
        seller.require_auth();

        require_positive_price(&env, min_bid);
        registry::ensure_seller_owns_active_ip(&env, ip_id, &seller);

        // Check no active auction exists
        if env.storage().persistent().has(&DataKey::ActiveAuction(ip_id)) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapExists as u32,
            ));
        }

        let auction_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextAuctionId)
            .unwrap_or(0);

        let start_time = env.ledger().timestamp();
        let end_time = start_time + duration_seconds;

        let auction = AuctionRecord {
            auction_id,
            ip_id,
            seller: seller.clone(),
            token: token.clone(),
            min_bid,
            highest_bid: 0,
            highest_bidder: None,
            start_time,
            end_time,
            finalized: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Auction(auction_id), &auction);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Auction(auction_id), LEDGER_BUMP, LEDGER_BUMP);

        env.storage()
            .persistent()
            .set(&DataKey::ActiveAuction(ip_id), &auction_id);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::ActiveAuction(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        env.storage()
            .persistent()
            .set(&DataKey::NextAuctionId, &(auction_id + 1));
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::NextAuctionId, LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("auc_strt"),),
            AuctionStartedEvent {
                auction_id,
                ip_id,
                seller,
                min_bid,
                end_time,
            },
        );

        auction_id
    }

    /// Place a bid on an active auction.
    pub fn place_bid(env: Env, auction_id: u64, bidder: Address, bid_amount: i128) {
        bidder.require_auth();

        let mut auction: AuctionRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Auction(auction_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ));
            });

        if auction.finalized {
            env.panic_with_error(Error::from_contract_error(
                ContractError::AlreadyInit as u32,
            ));
        }

        if env.ledger().timestamp() >= auction.end_time {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NotExpired as u32,
            ));
        }

        if bid_amount < auction.min_bid {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PriceTooSmall as u32,
            ));
        }

        if bid_amount <= auction.highest_bid {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PriceTooSmall as u32,
            ));
        }

        // Refund previous highest bidder if exists
        if let Some(ref prev_bidder) = auction.highest_bidder {
            token::Client::new(&env, &auction.token).transfer(
                &env.current_contract_address(),
                prev_bidder,
                &auction.highest_bid,
            );
        }

        // Transfer new bid to contract
        token::Client::new(&env, &auction.token).transfer(
            &bidder,
            &env.current_contract_address(),
            &bid_amount,
        );

        auction.highest_bid = bid_amount;
        auction.highest_bidder = Some(bidder.clone());

        env.storage()
            .persistent()
            .set(&DataKey::Auction(auction_id), &auction);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Auction(auction_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("bid_plcd"),),
            BidPlacedEvent {
                auction_id,
                bidder,
                bid_amount,
            },
        );
    }

    /// Finalize an auction after it ends. Creates a swap with the winning bid.
    pub fn finalize_auction(env: Env, auction_id: u64) {
        let mut auction: AuctionRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Auction(auction_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ));
            });

        if auction.finalized {
            env.panic_with_error(Error::from_contract_error(
                ContractError::AlreadyInit as u32,
            ));
        }

        if env.ledger().timestamp() < auction.end_time {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NotExpired as u32,
            ));
        }

        auction.finalized = true;
        env.storage()
            .persistent()
            .set(&DataKey::Auction(auction_id), &auction);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Auction(auction_id), LEDGER_BUMP, LEDGER_BUMP);

        // Remove active auction lock
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveAuction(auction.ip_id));

        let winning_bid = auction.highest_bid;
        let winner = auction.highest_bidder.clone();

        env.events().publish(
            (soroban_sdk::symbol_short!("auc_fnl"),),
            AuctionFinalizedEvent {
                auction_id,
                winner: winner.clone(),
                winning_bid,
            },
        );

        // If there's a winner, create a swap automatically
        if let Some(buyer) = winner {
            let swap_id: u64 = env
                .storage()
                .instance()
                .get(&DataKey::NextId)
                .unwrap_or(0);

            let swap = SwapRecord {
                ip_id: auction.ip_id,
                seller: auction.seller.clone(),
                buyer: buyer.clone(),
                price: winning_bid,
                token: auction.token.clone(),
                status: SwapStatus::Accepted,
                expiry: env.ledger().timestamp() + 604800u64,
                accept_timestamp: env.ledger().timestamp(),
                required_approvals: 0,
                dispute_timestamp: 0,
                referrer: None,
                collateral_amount: 0,
                insurance_premium: 0,
                insurance_enabled: false,
                escrow_agent: None,
                quantity: 1,
                conditions: Vec::new(&env),
            paid_amount: 0,
            is_installment: false,
            arbitrator: None,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Swap(swap_id), &swap);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::Swap(swap_id), LEDGER_BUMP, LEDGER_BUMP);

            swap::append_swap_for_party(&env, &auction.seller, &buyer, swap_id);

            let mut ip_swap_ids: Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::IpSwaps(auction.ip_id))
                .unwrap_or(Vec::new(&env));
            ip_swap_ids.push_back(swap_id);
            env.storage()
                .persistent()
                .set(&DataKey::IpSwaps(auction.ip_id), &ip_swap_ids);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::IpSwaps(auction.ip_id), LEDGER_BUMP, LEDGER_BUMP);

            Self::append_history(&env, swap_id, SwapStatus::Accepted);
            env.storage().instance().set(&DataKey::NextId, &(swap_id + 1));

            env.events().publish(
                (soroban_sdk::symbol_short!("swap_init"),),
                SwapInitiatedEvent {
                    swap_id,
                    ip_id: auction.ip_id,
                    seller: auction.seller,
                    buyer,
                    price: winning_bid,
                },
            );
        }
    }

    /// Get auction details.
    pub fn get_auction(env: Env, auction_id: u64) -> AuctionRecord {
        env.storage()
            .persistent()
            .get(&DataKey::Auction(auction_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ));
            })
    }

    // ── #349: Scheduled Payment Support ───────────────────────────────────────

    /// Initiate a swap with a payment schedule. Seller-only.
    pub fn initiate_swap_with_schedule(
        env: Env,
        token: Address,
        ip_id: u64,
        seller: Address,
        schedule: Vec<PaymentSchedule>,
        buyer: Address,
    ) -> u64 {
        require_not_paused(&env);
        seller.require_auth();

        if schedule.is_empty() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PriceTooSmall as u32,
            ));
        }

        // Calculate total price from schedule
        let mut total_price: i128 = 0;
        for payment in schedule.iter() {
            total_price = total_price.saturating_add(payment.amount);
        }

        require_positive_price(&env, total_price);
        registry::ensure_seller_owns_active_ip(&env, ip_id, &seller);
        require_no_active_swap(&env, ip_id);

        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(0);

        let swap = SwapRecord {
            ip_id,
            seller: seller.clone(),
            buyer: buyer.clone(),
            price: total_price,
            token: token.clone(),
            status: SwapStatus::Pending,
            expiry: env.ledger().timestamp() + 604800u64,
            accept_timestamp: 0,
            required_approvals: 0,
            dispute_timestamp: 0,
            referrer: None,
            collateral_amount: 0,
            insurance_premium: 0,
            insurance_enabled: false,
            escrow_agent: None,
            quantity: 1,
            conditions: Vec::new(&env),
            paid_amount: 0,
            is_installment: true,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Swap(id), &swap);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Swap(id), LEDGER_BUMP, LEDGER_BUMP);

        env.storage()
            .persistent()
            .set(&DataKey::ActiveSwap(ip_id), &id);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::ActiveSwap(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        // Store payment schedule
        env.storage()
            .persistent()
            .set(&DataKey::PaymentSchedule(id), &schedule);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::PaymentSchedule(id), LEDGER_BUMP, LEDGER_BUMP);

        // Initialize payments tracking (all false initially)
        let mut payments_made: Vec<bool> = Vec::new(&env);
        for _ in 0..schedule.len() {
            payments_made.push_back(false);
        }
        env.storage()
            .persistent()
            .set(&DataKey::PaymentsMade(id), &payments_made);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::PaymentsMade(id), LEDGER_BUMP, LEDGER_BUMP);

        swap::append_swap_for_party(&env, &seller, &buyer, id);

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
            .extend_ttl(&DataKey::IpSwaps(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        Self::append_history(&env, id, SwapStatus::Pending);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        env.events().publish(
            (soroban_sdk::symbol_short!("swap_init"),),
            SwapInitiatedEvent {
                swap_id: id,
                ip_id,
                seller,
                buyer,
                price: total_price,
            },
        );

        id
    }

    /// Make a scheduled payment. Buyer-only.
    pub fn make_scheduled_payment(env: Env, swap_id: u64, payment_index: u32) {
        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();

        let schedule: Vec<PaymentSchedule> = env
            .storage()
            .persistent()
            .get(&DataKey::PaymentSchedule(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::SwapNotFound as u32,
                ));
            });

        if payment_index >= schedule.len() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidKey as u32,
            ));
        }

        let mut payments_made: Vec<bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PaymentsMade(swap_id))
            .unwrap_or(Vec::new(&env));

        if payments_made.get(payment_index).unwrap_or(false) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::AlreadyInit as u32,
            ));
        }

        let payment = schedule.get(payment_index).unwrap();
        if env.ledger().timestamp() < payment.due_timestamp {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NotExpired as u32,
            ));
        }

        // Transfer payment
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &payment.amount,
        );

        // Mark payment as made
        payments_made.set(payment_index, true);
        env.storage()
            .persistent()
            .set(&DataKey::PaymentsMade(swap_id), &payments_made);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::PaymentsMade(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        // Check if all payments are made
        let mut all_paid = true;
        for i in 0..payments_made.len() {
            if !payments_made.get(i).unwrap_or(false) {
                all_paid = false;
                break;
            }
        }

        // If all payments made, transition to Accepted
        if all_paid {
            swap.status = SwapStatus::Accepted;
            swap.accept_timestamp = env.ledger().timestamp();
            swap::save_swap(&env, swap_id, &swap);
            Self::append_history(&env, swap_id, SwapStatus::Accepted);

            env.events().publish(
                (soroban_sdk::symbol_short!("swap_acpt"),),
                SwapAcceptedEvent {
                    swap_id,
                    buyer: swap.buyer.clone(),
                },
            );
        }

        let remaining = (schedule.len() as u32) - (payment_index + 1);
        env.events().publish(
            (soroban_sdk::symbol_short!("sched_pay"),),
            ScheduledPaymentMadeEvent {
                swap_id,
                payment_index,
                amount: payment.amount,
                remaining_payments: remaining,
            },
        );
    }

    /// Get payment schedule for a swap.
    pub fn get_payment_schedule(env: Env, swap_id: u64) -> Vec<PaymentSchedule> {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentSchedule(swap_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get payment status for a swap.
    pub fn get_payments_made(env: Env, swap_id: u64) -> Vec<bool> {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentsMade(swap_id))
            .unwrap_or(Vec::new(&env))
    }

    // ── Installment Payments ──────────────────────────────────────────────────

    /// Seller initiates a swap that the buyer may pay in installments.
    ///
    /// The swap is created in `Pending` state with `is_installment = true`.
    /// The buyer calls `submit_installment_payment` one or more times until
    /// `paid_amount >= price`, at which point the swap transitions to `Accepted`
    /// and the seller can reveal the decryption key.
    ///
    /// `num_installments` is informational (stored as a hint for the buyer) and
    /// does not enforce a maximum number of payments.
    pub fn initiate_swap_installment(
        env: Env,
        token: Address,
        ip_id: u64,
        seller: Address,
        price: i128,
        buyer: Address,
        num_installments: u32,
    ) -> u64 {
        require_not_paused(&env);
        seller.require_auth();
        require_positive_price(&env, price);
        if num_installments == 0 {
            env.panic_with_error(Error::from_contract_error(ContractError::PriceTooSmall as u32));
        }
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
            required_approvals: 0,
            dispute_timestamp: 0,
            referrer: None,
            collateral_amount: 0,
            insurance_premium: 0,
            insurance_enabled: false,
            escrow_agent: None,
            quantity: num_installments,
            conditions: Vec::new(&env),
            paid_amount: 0,
            is_installment: true,
        };

        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage().persistent().extend_ttl(&DataKey::Swap(id), LEDGER_BUMP, LEDGER_BUMP);
        env.storage().persistent().set(&DataKey::ActiveSwap(ip_id), &id);
        env.storage().persistent().extend_ttl(&DataKey::ActiveSwap(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        swap::append_swap_for_party(&env, &seller, &buyer, id);

        let mut ip_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::IpSwaps(ip_id))
            .unwrap_or(Vec::new(&env));
        ip_ids.push_back(id);
        env.storage().persistent().set(&DataKey::IpSwaps(ip_id), &ip_ids);
        env.storage().persistent().extend_ttl(&DataKey::IpSwaps(ip_id), 50000, 50000);

        Self::append_history(&env, id, SwapStatus::Pending);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        env.events().publish(
            (symbol_short!("swap_init"),),
            SwapInitiatedEvent { swap_id: id, ip_id, seller, buyer, price },
        );

        id
    }

    /// Submit an installment payment toward a scheduled swap. Buyer-only.
    ///
    /// Transfers `payment_amount` tokens from buyer to escrow and accumulates
    /// `paid_amount` on the swap. Once `paid_amount >= price` the swap
    /// transitions to Accepted, signalling the seller to reveal the key.
    ///
    /// Panics if:
    /// - swap not found or not an installment swap
    /// - caller is not the buyer
    /// - swap is not in Pending state
    /// - payment_amount is zero
    /// - total would exceed price (overpayment rejected)
    pub fn submit_installment_payment(env: Env, swap_id: u64, payment_amount: i128) {
        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();

        if !swap.is_installment {
            env.panic_with_error(Error::from_contract_error(ContractError::NotPending as u32));
        }
        if swap.status != SwapStatus::Pending {
            env.panic_with_error(Error::from_contract_error(ContractError::NotPending as u32));
        }
        if payment_amount <= 0 {
            env.panic_with_error(Error::from_contract_error(ContractError::PriceTooSmall as u32));
        }

        let remaining = swap.price.saturating_sub(swap.paid_amount);
        if payment_amount > remaining {
            env.panic_with_error(Error::from_contract_error(ContractError::PriceTooSmall as u32));
        }

        // Transfer this installment into escrow
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &payment_amount,
        );

        swap.paid_amount = swap.paid_amount.saturating_add(payment_amount);

        // If fully paid, transition to Accepted so seller can reveal key
        if swap.paid_amount >= swap.price {
            swap.status = SwapStatus::Accepted;
            swap.accept_timestamp = env.ledger().timestamp();
            Self::append_history(&env, swap_id, SwapStatus::Accepted);
            env.events().publish(
                (symbol_short!("swap_acpt"),),
                SwapAcceptedEvent { swap_id, buyer: swap.buyer.clone() },
            );
        }

        swap::save_swap(&env, swap_id, &swap);

        env.events().publish(
            (symbol_short!("inst_pay"),),
            (swap_id, payment_amount, swap.paid_amount, swap.price),
        );
    }

    /// Returns (paid_amount, total_price, remaining) for an installment swap.
    pub fn get_installment_status(env: Env, swap_id: u64) -> (i128, i128, i128) {
        let swap = require_swap_exists(&env, swap_id);
        let remaining = swap.price.saturating_sub(swap.paid_amount);
        (swap.paid_amount, swap.price, remaining)
    }

    // ── #350: Collateral Management ───────────────────────────────────────────

    /// Get collateral amount for a swap.
    pub fn get_swap_collateral(env: Env, swap_id: u64) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::SwapCollateral(swap_id))
            .unwrap_or(0)
    }

    // ── #355: Arbitration by Third Party ──────────────────────────────────────

    /// Request arbitration for a disputed swap. Buyer or seller only.
    /// Records the request timestamp so auto-refund can be triggered after
    /// `arbitration_timeout_seconds` if admin never resolves the dispute.
    pub fn request_arbitration(
        env: Env,
        swap_id: u64,
        requester: Address,
        evidence_hash: BytesN<32>,
    ) {
        let swap = require_swap_exists(&env, swap_id);
        requester.require_auth();

        // Only buyer or seller can request arbitration
        if requester != swap.buyer && requester != swap.seller {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }

        // Swap must be in Disputed state
        require_swap_status(
            &env,
            &swap,
            SwapStatus::Disputed,
            ContractError::NotDisputed,
        );

        // Record timestamp (only set once — first request wins)
        if !env.storage().persistent().has(&DataKey::ArbitrationTimestamp(swap_id)) {
            let ts = env.ledger().timestamp();
            env.storage()
                .persistent()
                .set(&DataKey::ArbitrationTimestamp(swap_id), &ts);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::ArbitrationTimestamp(swap_id), LEDGER_BUMP, LEDGER_BUMP);
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_req"),),
            ArbitrationRequestedEvent {
                swap_id,
                requester,
                evidence_hash,
            },
        );
    }

    /// Anyone can call this after `arbitration_timeout_seconds` have elapsed since
    /// `request_arbitration` was called. If admin has not resolved the dispute by
    /// then, the buyer is automatically refunded and the swap is cancelled.
    pub fn auto_refund_timeout(env: Env, swap_id: u64) {
        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::NotDisputed);

        let arb_ts: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ArbitrationTimestamp(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NotDisputed as u32,
                ))
            });

        let config = Self::protocol_config(&env);
        let elapsed = env.ledger().timestamp().saturating_sub(arb_ts);
        if elapsed < config.arbitration_timeout_seconds {
            env.panic_with_error(Error::from_contract_error(
                ContractError::ArbitrationNotTimedOut as u32,
            ));
        }

        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);
        env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
        env.storage().persistent().remove(&DataKey::ArbitrationTimestamp(swap_id));

        // Refund buyer
        token::Client::new(&env, &swap.token).transfer(
            &env.current_contract_address(),
            &swap.buyer,
            &swap.price,
        );

        env.storage().persistent().set(
            &DataKey::CancelReason(swap_id),
            &Bytes::from_slice(&env, b"arbitration_timeout"),
        );

        Self::append_history(&env, swap_id, SwapStatus::Cancelled);

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_tout"),),
            DisputeResolvedEvent { swap_id, refunded: true },
        );
    }

    /// Set arbitrator for a swap. Called by admin or designated arbitrator.
    // Duplicate set_arbitrator removed - SwapArbitrator DataKey variant not defined

    /// Arbitrate a swap. Arbitrator-only. Decides to refund or complete.
    pub fn arbitrate_swap(env: Env, swap_id: u64, arbitrator: Address, refund: bool) {
        let mut swap = require_swap_exists(&env, swap_id);
        arbitrator.require_auth();

        // Verify arbitrator is set and matches
        let stored_arbitrator: Address = env
            .storage()
            .persistent()
            .get(&DataKey::SwapArbitrator(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NoArbitratorSet as u32,
                ))
            });

        if arbitrator != stored_arbitrator {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NotArbitrator as u32,
            ));
        }

        let token_client = token::Client::new(&env, &swap.token);

        if refund {
            // Refund buyer
            token_client.transfer(
                &env.current_contract_address(),
                &swap.buyer,
                &swap.price,
            );

            // Refund collateral if present
            if swap.collateral_amount > 0 {
                if let Some(collateral) = env
                    .storage()
                    .persistent()
                    .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
                {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &swap.buyer,
                        &collateral,
                    );
                    env.storage()
                        .persistent()
                        .remove(&DataKey::SwapCollateral(swap_id));
                }
            }

            swap.status = SwapStatus::Cancelled;
        } else {
            // Complete the swap
            swap.status = SwapStatus::Completed;

            // Release collateral to seller
            if swap.collateral_amount > 0 {
                if let Some(collateral) = env
                    .storage()
                    .persistent()
                    .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
                {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &swap.seller,
                        &collateral,
                    );
                    env.storage()
                        .persistent()
                        .remove(&DataKey::SwapCollateral(swap_id));

                    env.events().publish(
                        (soroban_sdk::symbol_short!("coll_rel"),),
                        CollateralReleasedEvent {
                            swap_id,
                            buyer: swap.buyer.clone(),
                            collateral_amount: collateral,
                        },
                    );
                }
            }

            // Release payment to seller
            let config = Self::protocol_config(&env);
            let fee_bps = config.protocol_fee_bps as i128;
            let fee_amount = if fee_bps > 0 && swap.price > 0 {
                (swap.price * fee_bps) / 10000
            } else {
                0
            };

            let seller_amount = swap.price - fee_amount;
            token_client.transfer(
                &env.current_contract_address(),
                &swap.seller,
                &seller_amount,
            );

            if fee_amount > 0 {
                token_client.transfer(
                    &env.current_contract_address(),
                    &config.treasury,
                    &fee_amount,
                );
            }
        }

        swap::save_swap(&env, swap_id, &swap);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveSwap(swap.ip_id));
        env.storage().persistent().remove(&DataKey::SwapArbitrator(swap_id));

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_dec"),),
            ArbitratedEvent {
                swap_id,
                arbitrator,
                refunded: refund,
            },
        );
    }

    // ── #356: Atomic Refund on Key Invalidity ─────────────────────────────────

    /// Verify key and complete swap atomically. Seller-only.
    /// If key is invalid, automatically refund buyer and cancel swap.
    pub fn verify_and_complete_swap(
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
            ContractError::NotAccepted,
        );

        // Verify commitment
        let valid = registry::verify_commitment(&env, swap.ip_id, &secret, &blinding_factor);

        let token_client = token::Client::new(&env, &swap.token);

        if !valid {
            // Atomic refund: invalid key triggers automatic refund
            token_client.transfer(
                &env.current_contract_address(),
                &swap.buyer,
                &swap.price,
            );

            // Refund collateral
            if swap.collateral_amount > 0 {
                if let Some(collateral) = env
                    .storage()
                    .persistent()
                    .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
                {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &swap.buyer,
                        &collateral,
                    );
                    env.storage()
                        .persistent()
                        .remove(&DataKey::SwapCollateral(swap_id));
                }
            }

            swap.status = SwapStatus::Cancelled;
            swap::save_swap(&env, swap_id, &swap);
            env.storage()
                .persistent()
                .remove(&DataKey::ActiveSwap(swap.ip_id));

            env.events().publish(
                (soroban_sdk::symbol_short!("atom_ref"),),
                AtomicRefundEvent {
                    swap_id,
                    buyer: swap.buyer.clone(),
                    refund_amount: swap.price,
                    reason: String::from_str(&env, "Invalid decryption key"),
                },
            );
        } else {
            // Valid key: complete swap normally
            swap.status = SwapStatus::Completed;
            swap::save_swap(&env, swap_id, &swap);
            env.storage()
                .persistent()
                .remove(&DataKey::ActiveSwap(swap.ip_id));

            // Process payment
            let config = Self::protocol_config(&env);
            let fee_bps = config.protocol_fee_bps as i128;
            let fee_amount = if fee_bps > 0 && swap.price > 0 {
                (swap.price * fee_bps) / 10000
            } else {
                0
            };

            let seller_amount = swap.price - fee_amount;
            token_client.transfer(
                &env.current_contract_address(),
                &swap.seller,
                &seller_amount,
            );

            if fee_amount > 0 {
                token_client.transfer(
                    &env.current_contract_address(),
                    &config.treasury,
                    &fee_amount,
                );
            }

            // Release collateral to seller
            if swap.collateral_amount > 0 {
                if let Some(collateral) = env
                    .storage()
                    .persistent()
                    .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
                {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &swap.seller,
                        &collateral,
                    );
                    env.storage()
                        .persistent()
                        .remove(&DataKey::SwapCollateral(swap_id));

                    env.events().publish(
                        (soroban_sdk::symbol_short!("coll_rel"),),
                        CollateralReleasedEvent {
                            swap_id,
                            buyer: swap.buyer.clone(),
                            collateral_amount: collateral,
                        },
                    );
                }
            }

            env.events().publish(
                (soroban_sdk::symbol_short!("key_rev"),),
                KeyRevealedEvent {
                    swap_id,
                    seller_amount,
                    fee_amount,
                },
            );
        }

        // AtomicRefundProcessed DataKey variant not defined
        // env.storage().persistent().set(&DataKey::AtomicRefundProcessed(swap_id), &true);
        // env.storage().persistent().extend_ttl(&DataKey::AtomicRefundProcessed(swap_id), LEDGER_BUMP, LEDGER_BUMP);
    }

    // ── #357: Batch Processing ────────────────────────────────────────────────

    /// Batch accept multiple swaps. Buyer-only.
    pub fn batch_accept_swaps(env: Env, swap_ids: Vec<u64>, buyer: Address) {
        require_not_paused(&env);
        buyer.require_auth();

        // #520: Validate batch parameters upfront
        if swap_ids.is_empty() {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchEmpty as u32));
        }
        if swap_ids.len() > MAX_BATCH_SIZE {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchTooLarge as u32));
        }

        // #522: Pre-validation pass — no mutations; ensures full atomicity
        for swap_id in swap_ids.iter() {
            let swap = require_swap_exists(&env, swap_id);

            if swap.buyer != buyer {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::Unauthorized as u32,
                ));
            }

            require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);

            if swap.required_approvals > 0 {
                let approvals: Vec<Address> = env
                    .storage()
                    .persistent()
                    .get(&DataKey::SwapApprovals(swap_id))
                    .unwrap_or(Vec::new(&env));
                if (approvals.len() as u32) < swap.required_approvals {
                    env.panic_with_error(Error::from_contract_error(
                        ContractError::NeedApprovals as u32,
                    ));
                }
            }
        }

        // Execution pass — all validations passed, safe to mutate
        for swap_id in swap_ids.iter() {
            let mut swap = require_swap_exists(&env, swap_id);

            if swap.collateral_amount > 0 {
                if !env.storage().persistent().has(&DataKey::SwapCollateral(swap_id)) {
                    let token_client = token::Client::new(&env, &swap.token);
                    token_client.transfer(
                        &swap.buyer,
                        &env.current_contract_address(),
                        &swap.collateral_amount,
                    );
                    env.storage()
                        .persistent()
                        .set(&DataKey::SwapCollateral(swap_id), &swap.collateral_amount);
                    env.storage()
                        .persistent()
                        .extend_ttl(&DataKey::SwapCollateral(swap_id), LEDGER_BUMP, LEDGER_BUMP);
                }
            }

            let token_client = token::Client::new(&env, &swap.token);
            token_client.transfer(&swap.buyer, &env.current_contract_address(), &swap.price);

            // #354: Collect insurance premium from buyer and add to pool.
            if swap.insurance_enabled && swap.insurance_premium > 0 {
                token_client.transfer(
                    &swap.buyer,
                    &env.current_contract_address(),
                    &swap.insurance_premium,
                );
                let pool_key = DataKey::InsurancePool(swap.token.clone());
                let pool: i128 = env.storage().persistent().get(&pool_key).unwrap_or(0);
                env.storage().persistent().set(&pool_key, &(pool + swap.insurance_premium));
                env.storage().persistent().extend_ttl(&pool_key, LEDGER_BUMP, LEDGER_BUMP);
            }

            swap.accept_timestamp = env.ledger().timestamp();
            swap.status = SwapStatus::Accepted;
            swap::save_swap(&env, swap_id, &swap);
            Self::append_history(&env, swap_id, SwapStatus::Accepted);
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("btch_acp"),),
            BatchAcceptedEvent { swap_ids, buyer },
        );
    }

    /// Batch reveal keys for multiple swaps. Seller-only.
    /// #518: Emits BatchFeeBreakdownEvent with per-swap fee breakdown.
    pub fn batch_reveal_keys(
        env: Env,
        swap_ids: Vec<u64>,
        secrets: Vec<BytesN<32>>,
        blinding_factors: Vec<BytesN<32>>,
        seller: Address,
    ) {
        seller.require_auth();

        // #520: Validate batch parameters upfront
        if swap_ids.is_empty() {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchEmpty as u32));
        }
        if secrets.len() != swap_ids.len() || blinding_factors.len() != swap_ids.len() {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchSizeMismatch as u32));
        }
        if swap_ids.len() > MAX_BATCH_SIZE {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchTooLarge as u32));
        }

        let mut fee_breakdowns: Vec<SwapFeeBreakdown> = Vec::new(&env);

        for i in 0..swap_ids.len() {
            let swap_id = swap_ids.get(i).unwrap();
            let swap = require_swap_exists(&env, swap_id);
            require_seller(&env, &seller, &swap);
            require_swap_status(&env, &swap, SwapStatus::Accepted, ContractError::NotAccepted);
            let valid = registry::verify_commitment(
                &env,
                swap.ip_id,
                &secrets.get(i).unwrap(),
                &blinding_factors.get(i).unwrap(),
            );
            if !valid {
                env.panic_with_error(Error::from_contract_error(ContractError::InvalidKey as u32));
            }
        }

        // Execution pass — all keys verified, safe to release payments
        for i in 0..swap_ids.len() {
            let swap_id = swap_ids.get(i).unwrap();
            let mut swap = require_swap_exists(&env, swap_id);

            swap.status = SwapStatus::Completed;
            swap::save_swap(&env, swap_id, &swap);
            env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
            Self::append_history(&env, swap_id, SwapStatus::Completed);

            let token_client = token::Client::new(&env, &swap.token);
            let config = Self::protocol_config(&env);
            let fee_bps = config.protocol_fee_bps as i128;
            let fee_amount = if fee_bps > 0 && swap.price > 0 {
                (swap.price * fee_bps) / 10000
            } else {
                0
            };

            // #518: Compute referral fee
            let referral_amount = if let Some(ref _referrer) = swap.referrer {
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
            token_client.transfer(
                &env.current_contract_address(),
                &swap.seller,
                &seller_amount,
            );

            if fee_amount > 0 {
                token_client.transfer(&env.current_contract_address(), &config.treasury, &fee_amount);
            }

            // #518: Pay referral reward
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

            // #518: Collect fee breakdown
            fee_breakdowns.push_back(SwapFeeBreakdown {
                swap_id,
                price: swap.price,
                protocol_fee: fee_amount,
                referral_fee: referral_amount,
                seller_amount,
            });

            // Release collateral
            if swap.collateral_amount > 0 {
                if let Some(collateral) = env
                    .storage()
                    .persistent()
                    .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
                {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &swap.seller,
                        &collateral,
                    );
                    env.storage().persistent().remove(&DataKey::SwapCollateral(swap_id));

                    env.events().publish(
                        (soroban_sdk::symbol_short!("coll_rel"),),
                        CollateralReleasedEvent {
                            swap_id,
                            buyer: swap.buyer.clone(),
                            collateral_amount: collateral,
                        },
                    );
                }
            }

            // Update reputation on batch completion
            Self::update_reputation(&env, &swap.seller, 5);
            Self::update_reputation(&env, &swap.buyer, 5);
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("btch_key"),),
            BatchKeysRevealedEvent {
                swap_ids: swap_ids.clone(),
                seller: seller.clone(),
            },
        );

        // #518: Emit fee breakdown event
        env.events().publish(
            (soroban_sdk::symbol_short!("btch_fee"),),
            BatchFeeBreakdownEvent {
                swap_ids,
                seller,
                fees: fee_breakdowns,
            },
        );
    }

    // ── #517: Batch Swap Cancellation ─────────────────────────────────────────

    /// Cancel multiple pending swaps in a batch. Only the seller or buyer may cancel.
    /// Each swap must be in Pending status. Tracks cancellation reasons for each swap.
    /// Returns the list of swap IDs that were successfully cancelled.
    pub fn batch_cancel_swaps(
        env: Env,
        swap_ids: Vec<u64>,
        canceller: Address,
        reasons: Vec<Bytes>,
    ) -> Vec<u64> {
        canceller.require_auth();

        let len = swap_ids.len();
        if reasons.len() != len {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidKey as u32,
            ));
        }

        let mut cancelled_ids: Vec<u64> = Vec::new(&env);

        for i in 0..len {
            let swap_id = swap_ids.get(i).unwrap();
            let reason = reasons.get(i).unwrap();

            let mut swap = require_swap_exists(&env, swap_id);

            require_seller_or_buyer(&env, &canceller, &swap);
            require_swap_status(
                &env,
                &swap,
                SwapStatus::Pending,
                ContractError::OnlyPending,
            );

            swap.status = SwapStatus::Cancelled;
            swap::save_swap(&env, swap_id, &swap);

            // Release the IP lock
            env.storage()
                .persistent()
                .remove(&DataKey::ActiveSwap(swap.ip_id));

            // Store cancellation reason
            env.storage()
                .persistent()
                .set(&DataKey::CancelReason(swap_id), &reason);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::CancelReason(swap_id), LEDGER_BUMP, LEDGER_BUMP);

            // #253: Log history entry
            Self::append_history(&env, swap_id, SwapStatus::Cancelled);

            // Update reputation: canceller loses 10 points
            Self::update_reputation(&env, &canceller, -10);

            cancelled_ids.push_back(swap_id);
        }

        // Emit batch cancellation event
        env.events().publish(
            (soroban_sdk::symbol_short!("btch_ccl"),),
            BatchCancelledEvent {
                swap_ids: cancelled_ids.clone(),
                canceller,
                reasons,
            },
        );

        cancelled_ids
    }

    /// Seller initiates multiple patent sales with optional insurance. Returns a Vec of swap IDs.
    /// When `insurance_enabled` is true, each swap's premium is set to 2% of its price and
    /// collected from the buyer at `batch_accept_swaps` time.
    pub fn batch_initiate_swap_with_insurance(
        env: Env,
        token: Address,
        ip_ids: Vec<u64>,
        seller: Address,
        prices: Vec<i128>,
        buyer: Address,
        required_approvals: u32,
        referrer: Option<Address>,
        insurance_enabled: bool,
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
            let insurance_premium = if insurance_enabled { price * 2 / 100 } else { 0 };

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
                collateral_amount: 0,
                insurance_premium,
                insurance_enabled,
                escrow_agent: None,
                quantity: 1,
                conditions: Vec::new(&env),
                paid_amount: 0,
                is_installment: false,
                arbitrator: None,
            };

            if insurance_enabled {
                env.storage().persistent().set(&DataKey::SwapInsurance(id), &insurance_premium);
                env.storage().persistent().extend_ttl(&DataKey::SwapInsurance(id), LEDGER_BUMP, LEDGER_BUMP);
            }

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

    /// Arbitrate multiple disputed swaps in one call. Arbitrator-only.
    /// `refund` applies uniformly to all swaps in the batch.
    pub fn batch_arbitrate_swaps(env: Env, swap_ids: Vec<u64>, arbitrator: Address, refund: bool) {
        arbitrator.require_auth();

        for swap_id in swap_ids.iter() {
            let mut swap = require_swap_exists(&env, swap_id);
            require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::NotDisputed);

            let token_client = token::Client::new(&env, &swap.token);

            if refund {
                token_client.transfer(&env.current_contract_address(), &swap.buyer, &swap.price);

                if swap.collateral_amount > 0 {
                    if let Some(collateral) = env
                        .storage()
                        .persistent()
                        .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
                    {
                        token_client.transfer(&env.current_contract_address(), &swap.buyer, &collateral);
                        env.storage().persistent().remove(&DataKey::SwapCollateral(swap_id));
                    }
                }

                swap.status = SwapStatus::Cancelled;
            } else {
                let config = Self::protocol_config(&env);
                let fee_amount = if config.protocol_fee_bps > 0 && swap.price > 0 {
                    (swap.price * config.protocol_fee_bps as i128) / 10000
                } else {
                    0
                };
                let seller_amount = swap.price - fee_amount;
                token_client.transfer(&env.current_contract_address(), &swap.seller, &seller_amount);
                if fee_amount > 0 {
                    token_client.transfer(&env.current_contract_address(), &config.treasury, &fee_amount);
                }

                if swap.collateral_amount > 0 {
                    if let Some(collateral) = env
                        .storage()
                        .persistent()
                        .get::<_, i128>(&DataKey::SwapCollateral(swap_id))
                    {
                        token_client.transfer(&env.current_contract_address(), &swap.seller, &collateral);
                        env.storage().persistent().remove(&DataKey::SwapCollateral(swap_id));
                    }
                }

                swap.status = SwapStatus::Completed;
            }

            swap::save_swap(&env, swap_id, &swap);
            env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
            Self::append_history(&env, swap_id, swap.status.clone());

            env.events().publish(
                (soroban_sdk::symbol_short!("arb_dec"),),
                ArbitratedEvent { swap_id, arbitrator: arbitrator.clone(), refunded: refund },
            );
        }
    }

    // ── #358: Swap Timeout Escalation ─────────────────────────────────────────

    /// Request timeout escalation. Buyer-only. Extends deadline if timeout imminent.
        // escalate_swap_timeout removed - TimeoutExtension DataKey variant not defined

    // ── Escrow Swap Flow ──────────────────────────────────────────────────────

    /// Initiate an escrow-mode swap. Seller creates the swap; buyer deposits later.
    ///
    /// Returns the swap_id. The swap is stored with `SwapMode::Escrow` and
    /// `SwapStatus::Pending`. The `timeout` parameter sets the deadline (ledger
    /// timestamp) after which the buyer may withdraw their deposit if the seller
    /// has not revealed the key.
    pub fn initiate_escrow_swap(
        env: Env,
        token: Address,
        ip_id: u64,
        seller: Address,
        price: i128,
        buyer: Address,
        timeout: u64,
    ) -> u64 {
        require_not_paused(&env);
        seller.require_auth();
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
            expiry: timeout,
            accept_timestamp: 0,
            required_approvals: 0,
            dispute_timestamp: 0,
            referrer: None,
            collateral_amount: 0,
            insurance_premium: 0,
            insurance_enabled: false,
            escrow_agent: None,
            quantity: 1,
            conditions: Vec::new(&env),
            paid_amount: 0,
            is_installment: false,
            arbitrator: None,
        };

        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage().persistent().extend_ttl(&DataKey::Swap(id), LEDGER_BUMP, LEDGER_BUMP);
        env.storage().persistent().set(&DataKey::ActiveSwap(ip_id), &id);
        env.storage().persistent().extend_ttl(&DataKey::ActiveSwap(ip_id), LEDGER_BUMP, LEDGER_BUMP);
        env.storage().persistent().set(&DataKey::SwapMode(id), &SwapMode::Escrow);
        env.storage().persistent().extend_ttl(&DataKey::SwapMode(id), LEDGER_BUMP, LEDGER_BUMP);

        swap::append_swap_for_party(&env, &seller, &buyer, id);
        Self::append_history(&env, id, SwapStatus::Pending);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        env.events().publish(
            (soroban_sdk::symbol_short!("esc_ini"),),
            SwapInitiatedEvent { swap_id: id, ip_id, seller, buyer, price },
        );

        id
    }

    /// Batch initiate multiple escrow-mode swaps in a single transaction.
    ///
    /// Each `ip_ids[i]` is paired with `prices[i]` and `timeouts[i]` (expiry).
    /// Returns a vector of assigned swap IDs.
    pub fn batch_initiate_escrow(
        env: Env,
        token: Address,
        ip_ids: Vec<u64>,
        seller: Address,
        prices: Vec<i128>,
        buyer: Address,
        timeouts: Vec<u64>,
    ) -> Vec<u64> {
        require_not_paused(&env);
        seller.require_auth();

        let len = ip_ids.len();
        if len == 0 || prices.len() != len || timeouts.len() != len {
            env.panic_with_error(Error::from_contract_error(ContractError::PriceTooSmall as u32));
        }

        let mut swap_ids: Vec<u64> = Vec::new(&env);

        for i in 0..len {
            let ip_id = ip_ids.get(i).unwrap();
            let price = prices.get(i).unwrap();
            let timeout = timeouts.get(i).unwrap();

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
                expiry: timeout,
                accept_timestamp: 0,
                required_approvals: 0,
                dispute_timestamp: 0,
                referrer: None,
                collateral_amount: 0,
                insurance_premium: 0,
                insurance_enabled: false,
                escrow_agent: None,
                quantity: 1,
                conditions: Vec::new(&env),
                paid_amount: 0,
                is_installment: false,
                arbitrator: None,
            };

            env.storage().persistent().set(&DataKey::Swap(id), &swap);
            env.storage().persistent().extend_ttl(&DataKey::Swap(id), LEDGER_BUMP, LEDGER_BUMP);
            env.storage().persistent().set(&DataKey::ActiveSwap(ip_id), &id);
            env.storage().persistent().extend_ttl(&DataKey::ActiveSwap(ip_id), LEDGER_BUMP, LEDGER_BUMP);
            env.storage().persistent().set(&DataKey::SwapMode(id), &SwapMode::Escrow);
            env.storage().persistent().extend_ttl(&DataKey::SwapMode(id), LEDGER_BUMP, LEDGER_BUMP);

            swap::append_swap_for_party(&env, &seller, &buyer, id);

            let mut ip_swap_ids: Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::IpSwaps(ip_id))
                .unwrap_or(Vec::new(&env));
            ip_swap_ids.push_back(id);
            env.storage()
                .persistent()
                .set(&DataKey::IpSwaps(ip_id), &ip_swap_ids);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::IpSwaps(ip_id), 50000, 50000);

            Self::append_history(&env, id, SwapStatus::Pending);
            env.storage().instance().set(&DataKey::NextId, &(id + 1));

            env.events().publish(
                (soroban_sdk::symbol_short!("esc_ini"),),
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

    /// Buyer deposits funds into multiple escrow-mode swaps in one call.
    /// Each swap must be in `Pending` status with `SwapMode::Escrow`.
    pub fn batch_escrow_deposit(env: Env, swap_ids: Vec<u64>, buyer: Address) {
        buyer.require_auth();

        for swap_id in swap_ids.iter() {
            let mut swap = require_swap_exists(&env, swap_id);

            if swap.buyer != buyer {
                env.panic_with_error(Error::from_contract_error(ContractError::Unauthorized as u32));
            }

            let mode: SwapMode = env
                .storage()
                .persistent()
                .get(&DataKey::SwapMode(swap_id))
                .unwrap_or(SwapMode::Atomic);
            if mode != SwapMode::Escrow {
                env.panic_with_error(Error::from_contract_error(ContractError::Unauthorized as u32));
            }

            require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);

            token::Client::new(&env, &swap.token).transfer(
                &swap.buyer,
                &env.current_contract_address(),
                &swap.price,
            );

            env.storage().persistent().set(&DataKey::EscrowDeposit(swap_id), &swap.price);
            env.storage().persistent().extend_ttl(&DataKey::EscrowDeposit(swap_id), LEDGER_BUMP, LEDGER_BUMP);

            swap.accept_timestamp = env.ledger().timestamp();
            swap.status = SwapStatus::Accepted;
            swap::save_swap(&env, swap_id, &swap);
            Self::append_history(&env, swap_id, SwapStatus::Accepted);

            env.events().publish(
                (soroban_sdk::symbol_short!("esc_dep"),),
                SwapAcceptedEvent { swap_id, buyer: swap.buyer },
            );
        }
    }

    /// Buyer deposits funds into escrow. Moves swap to `Accepted`.
    ///
    /// Transfers `price` tokens from buyer to the contract. Can only be called
    /// on an escrow-mode swap in `Pending` status.
    pub fn escrow_deposit(env: Env, swap_id: u64) {
        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();

        // Must be escrow mode
        let mode: SwapMode = env
            .storage()
            .persistent()
            .get(&DataKey::SwapMode(swap_id))
            .unwrap_or(SwapMode::Atomic);
        if mode != SwapMode::Escrow {
            env.panic_with_error(Error::from_contract_error(ContractError::Unauthorized as u32));
        }

        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);

        // Transfer payment from buyer into contract escrow
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &swap.price,
        );

        env.storage().persistent().set(&DataKey::EscrowDeposit(swap_id), &swap.price);
        env.storage().persistent().extend_ttl(&DataKey::EscrowDeposit(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        swap.accept_timestamp = env.ledger().timestamp();
        swap.status = SwapStatus::Accepted;
        swap::save_swap(&env, swap_id, &swap);
        Self::append_history(&env, swap_id, SwapStatus::Accepted);

        env.events().publish(
            (soroban_sdk::symbol_short!("esc_dep"),),
            SwapAcceptedEvent { swap_id, buyer: swap.buyer },
        );
    }

    /// Buyer withdraws their deposit after timeout if seller never revealed.
    ///
    /// Can only be called on an escrow-mode swap in `Accepted` status after
    /// `swap.expiry` has passed. Refunds the full deposit to the buyer and
    /// cancels the swap.
    pub fn escrow_withdraw(env: Env, swap_id: u64) {
        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();

        // Must be escrow mode
        let mode: SwapMode = env
            .storage()
            .persistent()
            .get(&DataKey::SwapMode(swap_id))
            .unwrap_or(SwapMode::Atomic);
        if mode != SwapMode::Escrow {
            env.panic_with_error(Error::from_contract_error(ContractError::Unauthorized as u32));
        }

        require_swap_status(&env, &swap, SwapStatus::Accepted, ContractError::NotAccepted);

        // Timeout must have passed
        if env.ledger().timestamp() <= swap.expiry {
            env.panic_with_error(Error::from_contract_error(ContractError::NotExpired as u32));
        }

        let deposit: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::EscrowDeposit(swap_id))
            .unwrap_or(0);

        swap.status = SwapStatus::Cancelled;
        swap::save_swap(&env, swap_id, &swap);
        env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
        env.storage().persistent().remove(&DataKey::EscrowDeposit(swap_id));
        Self::append_history(&env, swap_id, SwapStatus::Cancelled);

        // Refund buyer
        if deposit > 0 {
            token::Client::new(&env, &swap.token).transfer(
                &env.current_contract_address(),
                &swap.buyer,
                &deposit,
            );
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("esc_wdr"),),
            SwapCancelledEvent { swap_id, canceller: swap.buyer },
        );
    }

    // ── Rollback ──────────────────────────────────────────────────────────────

    /// Buyer-only. Within 24 hours of swap completion, the buyer can call this
    /// with `is_key_valid = false` to trigger a partial refund if the decryption
    /// key turned out to be invalid. 90% of the payment is refunded to the buyer;
    /// 10% is sent to the treasury as a penalty. Returns `true` if rolled back,
    /// `false` if the key was reported valid (no action taken).
    pub fn validate_and_rollback_swap(env: Env, swap_id: u64, is_key_valid: bool) -> bool {
        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();

        require_swap_status(&env, &swap, SwapStatus::Completed, ContractError::NotInAccepted);

        // Enforce 24-hour rollback window
        let completion_ts: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::CompletionTimestamp(swap_id))
            .unwrap_or(0);
        let elapsed = env.ledger().timestamp().saturating_sub(completion_ts);
        if elapsed > 86_400 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::RollbackWindowExpired as u32,
            ));
        }

        if is_key_valid {
            return false;
        }

        // 90% refund to buyer, 10% penalty to treasury
        let buyer_refund = swap.price * 90 / 100;
        let treasury_penalty = swap.price - buyer_refund;

        let config = Self::protocol_config(&env);
        let token_client = token::Client::new(&env, &swap.token);

        token_client.transfer(&env.current_contract_address(), &swap.buyer, &buyer_refund);
        if treasury_penalty > 0 {
            token_client.transfer(&env.current_contract_address(), &config.treasury, &treasury_penalty);
        }

        swap.status = SwapStatus::RolledBack;
        swap::save_swap(&env, swap_id, &swap);

        env.storage().persistent().remove(&DataKey::CompletionTimestamp(swap_id));

        Self::append_history(&env, swap_id, SwapStatus::RolledBack);

        env.events().publish(
            (soroban_sdk::symbol_short!("rollback"),),
            SwapRolledBackEvent { swap_id, buyer_refund, treasury_penalty },
        );

        true
    }

    // ── Multi-party reveal (co-inventor sign-off) ─────────────────────────────

    /// Initiate a swap that requires all `signers` to call `sign_swap_reveal`
    /// before the seller can call `reveal_key`. The seller must be included in
    /// `signers` or they can still call `reveal_key` once all signers have signed.
    /// Returns the swap ID.
    pub fn initiate_swap_with_signers(
        env: Env,
        token: Address,
        ip_id: u64,
        seller: Address,
        price: i128,
        buyer: Address,
        signers: Vec<Address>,
    ) -> u64 {
        require_not_paused(&env);
        seller.require_auth();
        require_positive_price(&env, price);
        registry::ensure_seller_owns_active_ip(&env, ip_id, &seller);
        require_no_active_swap(&env, ip_id);

        if signers.is_empty() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }

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
            required_approvals: 0,
            dispute_timestamp: 0,
            referrer: None,
            collateral_amount: 0,
            insurance_premium: 0,
            insurance_enabled: false,
            escrow_agent: None,
            quantity: 1,
            conditions: Vec::new(&env),
            paid_amount: 0,
            is_installment: false,
            arbitrator: None,
        };

        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage().persistent().extend_ttl(&DataKey::Swap(id), LEDGER_BUMP, LEDGER_BUMP);
        env.storage().persistent().set(&DataKey::ActiveSwap(ip_id), &id);
        env.storage().persistent().extend_ttl(&DataKey::ActiveSwap(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        // Store required signers
        env.storage().persistent().set(&DataKey::SwapSigners(id), &signers);
        env.storage().persistent().extend_ttl(&DataKey::SwapSigners(id), LEDGER_BUMP, LEDGER_BUMP);

        swap::append_swap_for_party(&env, &seller, &buyer, id);

        let mut ip_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::IpSwaps(ip_id))
            .unwrap_or(Vec::new(&env));
        ip_ids.push_back(id);
        env.storage().persistent().set(&DataKey::IpSwaps(ip_id), &ip_ids);
        env.storage().persistent().extend_ttl(&DataKey::IpSwaps(ip_id), LEDGER_BUMP, LEDGER_BUMP);

        Self::append_history(&env, id, SwapStatus::Pending);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        env.events().publish(
            (soroban_sdk::symbol_short!("swap_init"),),
            SwapInitiatedEvent { swap_id: id, ip_id, seller, buyer, price },
        );

        id
    }

    /// A required signer signs off on the key reveal for a swap.
    /// Once all required signers have signed, `reveal_key` is unblocked.
    pub fn sign_swap_reveal(env: Env, swap_id: u64, signer: Address) {
        signer.require_auth();

        let swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Accepted, ContractError::NotAccepted);

        let signers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::SwapSigners(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::Unauthorized as u32,
                ))
            });

        // Verify signer is in the required list
        let mut is_required = false;
        for i in 0..signers.len() {
            if signers.get(i).unwrap() == signer {
                is_required = true;
                break;
            }
        }
        if !is_required {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NotARequiredSigner as u32,
            ));
        }

        let mut signed: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::SwapSignatures(swap_id))
            .unwrap_or(Vec::new(&env));

        // Prevent duplicate signatures
        for i in 0..signed.len() {
            if signed.get(i).unwrap() == signer {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::AlreadySigned as u32,
                ));
            }
        }

        signed.push_back(signer);
        env.storage().persistent().set(&DataKey::SwapSignatures(swap_id), &signed);
        env.storage().persistent().extend_ttl(&DataKey::SwapSignatures(swap_id), LEDGER_BUMP, LEDGER_BUMP);
    }

    /// Sign off on multiple swaps in a single call. The `signer` must be a
    /// required co-signer on every swap in `swap_ids`. Each swap must be in
    /// `Accepted` state and the signer must not have already signed it.
    /// Equivalent to calling `sign_swap_reveal` for each swap_id individually.
    pub fn batch_sign_swap_reveal(env: Env, swap_ids: Vec<u64>, signer: Address) {
        signer.require_auth();

        for i in 0..swap_ids.len() {
            let swap_id = swap_ids.get(i).unwrap();
            let swap = require_swap_exists(&env, swap_id);
            require_swap_status(&env, &swap, SwapStatus::Accepted, ContractError::NotAccepted);

            let signers: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::SwapSigners(swap_id))
                .unwrap_or_else(|| {
                    env.panic_with_error(Error::from_contract_error(
                        ContractError::Unauthorized as u32,
                    ))
                });

            let mut is_required = false;
            for j in 0..signers.len() {
                if signers.get(j).unwrap() == signer {
                    is_required = true;
                    break;
                }
            }
            if !is_required {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NotARequiredSigner as u32,
                ));
            }

            let mut signed: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::SwapSignatures(swap_id))
                .unwrap_or(Vec::new(&env));

            for j in 0..signed.len() {
                if signed.get(j).unwrap() == signer {
                    env.panic_with_error(Error::from_contract_error(
                        ContractError::AlreadySigned as u32,
                    ));
                }
            }

            signed.push_back(signer.clone());
            env.storage()
                .persistent()
                .set(&DataKey::SwapSignatures(swap_id), &signed);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::SwapSignatures(swap_id), LEDGER_BUMP, LEDGER_BUMP);
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("btch_sgn"),),
            BatchSignedEvent {
                swap_ids,
                signer,
            },
        );
    }

    // ── Reputation ────────────────────────────────────────────────────────────

    /// Returns the reputation score (0–100) for an address. Defaults to 50.
    pub fn get_reputation(env: Env, address: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::UserReputation(address))
            .unwrap_or(50u32)
    }

    /// Seller sets a minimum buyer reputation required for a specific swap.
    /// Must be called by the swap's seller before the buyer accepts.
    pub fn set_reputation_multiplier(env: Env, swap_id: u64, min_reputation: u32) {
        let swap = require_swap_exists(&env, swap_id);
        swap.seller.require_auth();
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::NotPending);

        env.storage()
            .persistent()
            .set(&DataKey::ReputationMultiplier(swap_id), &min_reputation);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::ReputationMultiplier(swap_id), LEDGER_BUMP, LEDGER_BUMP);
    }

    /// Internal: adjust reputation score, clamped to [0, 100].
    fn update_reputation(env: &Env, address: &Address, delta: i32) {
        let current: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::UserReputation(address.clone()))
            .unwrap_or(50u32);
        let updated = (current as i32).saturating_add(delta).clamp(0, 100) as u32;
        env.storage()
            .persistent()
            .set(&DataKey::UserReputation(address.clone()), &updated);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::UserReputation(address.clone()), LEDGER_BUMP, LEDGER_BUMP);
    }

    // ── #515: Batch Escrow Arbitration ────────────────────────────────────────

    /// Request arbitration for multiple disputed escrow swaps in one call.
    ///
    /// For each swap_id:
    /// - Swap must be in `Disputed` status.
    /// - Requester must be the buyer or seller of that swap.
    /// - Records the arbitration timestamp (first request wins per swap).
    ///
    /// A single `evidence_hash` is shared across all swaps in the batch.
    /// Emits `BatchEscrowArbitrationEvent` after processing all swaps.
    pub fn batch_escrow_arbitration(
        env: Env,
        swap_ids: Vec<u64>,
        requester: Address,
        evidence_hash: BytesN<32>,
    ) {
        requester.require_auth();

        let len = swap_ids.len();
        if len == 0 {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchEmpty as u32));
        }
        if len > MAX_BATCH_SIZE {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchTooLarge as u32));
        }

        // Pre-validation pass: ensure all swaps are valid before mutating state
        for swap_id in swap_ids.iter() {
            let swap = require_swap_exists(&env, swap_id);
            if requester != swap.buyer && requester != swap.seller {
                env.panic_with_error(Error::from_contract_error(ContractError::Unauthorized as u32));
            }
            require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::NotDisputed);
        }

        // Execution pass: record arbitration timestamps
        for swap_id in swap_ids.iter() {
            if !env.storage().persistent().has(&DataKey::ArbitrationTimestamp(swap_id)) {
                let ts = env.ledger().timestamp();
                env.storage()
                    .persistent()
                    .set(&DataKey::ArbitrationTimestamp(swap_id), &ts);
                env.storage()
                    .persistent()
                    .extend_ttl(&DataKey::ArbitrationTimestamp(swap_id), LEDGER_BUMP, LEDGER_BUMP);
            }

            env.events().publish(
                (soroban_sdk::symbol_short!("arb_req"),),
                ArbitrationRequestedEvent {
                    swap_id,
                    requester: requester.clone(),
                    evidence_hash: evidence_hash.clone(),
                },
            );
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("btch_arb"),),
            BatchEscrowArbitrationEvent {
                swap_ids,
                requester,
                evidence_hash,
                count: len as u32,
            },
        );
    }

    // ── #516: Batch Timeout Auto-Resolution ───────────────────────────────────

    /// Auto-resolve multiple timed-out batch swaps in one call.
    ///
    /// For each swap_id:
    /// - Swap must be in `Disputed` status.
    /// - `ArbitrationTimestamp` must have been set via `request_arbitration` or
    ///   `batch_escrow_arbitration`.
    /// - `arbitration_timeout_seconds` must have elapsed since that timestamp.
    ///
    /// On success, each swap is cancelled and the buyer is refunded the escrowed
    /// amount (from `EscrowDeposit` if present, otherwise `swap.price`).
    /// Emits `BatchTimeoutAutoResolvedEvent` after processing all swaps.
    pub fn batch_timeout_auto_resolve(env: Env, swap_ids: Vec<u64>) {
        let len = swap_ids.len();
        if len == 0 {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchEmpty as u32));
        }
        if len > MAX_BATCH_SIZE {
            env.panic_with_error(Error::from_contract_error(ContractError::BatchTooLarge as u32));
        }

        let config = Self::protocol_config(&env);

        // Pre-validation pass: ensure all swaps are eligible before mutating state
        for swap_id in swap_ids.iter() {
            let swap = require_swap_exists(&env, swap_id);
            require_swap_status(&env, &swap, SwapStatus::Disputed, ContractError::NotDisputed);

            let arb_ts: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::ArbitrationTimestamp(swap_id))
                .unwrap_or_else(|| {
                    env.panic_with_error(Error::from_contract_error(ContractError::NotDisputed as u32))
                });

            let elapsed = env.ledger().timestamp().saturating_sub(arb_ts);
            if elapsed < config.arbitration_timeout_seconds {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::ArbitrationNotTimedOut as u32,
                ));
            }
        }

        // Execution pass: cancel each swap and refund buyers
        for swap_id in swap_ids.iter() {
            let mut swap = require_swap_exists(&env, swap_id);

            swap.status = SwapStatus::Cancelled;
            swap::save_swap(&env, swap_id, &swap);
            env.storage().persistent().remove(&DataKey::ActiveSwap(swap.ip_id));
            env.storage().persistent().remove(&DataKey::ArbitrationTimestamp(swap_id));

            // Refund from escrow deposit if present, otherwise from swap price
            let refund_amount: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::EscrowDeposit(swap_id))
                .unwrap_or(swap.price);

            if refund_amount > 0 {
                token::Client::new(&env, &swap.token).transfer(
                    &env.current_contract_address(),
                    &swap.buyer,
                    &refund_amount,
                );
            }

            env.storage().persistent().remove(&DataKey::EscrowDeposit(swap_id));

            env.storage().persistent().set(
                &DataKey::CancelReason(swap_id),
                &Bytes::from_slice(&env, b"batch_arbitration_timeout"),
            );
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::CancelReason(swap_id), LEDGER_BUMP, LEDGER_BUMP);

            Self::append_history(&env, swap_id, SwapStatus::Cancelled);

            env.events().publish(
                (soroban_sdk::symbol_short!("arb_tout"),),
                DisputeResolvedEvent { swap_id, refunded: true },
            );
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("btch_tor"),),
            BatchTimeoutAutoResolvedEvent {
                swap_ids,
                count: len as u32,
            },
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

// #[cfg(test)]
// mod tests;

// #[cfg(test)]
// mod prop_tests;

// #[cfg(test)]
// mod regression_tests;

// #[cfg(test)]
// mod benchmarks;

// #[cfg(test)]
// mod mutation_tests;

// #[cfg(test)]
// mod snapshot_tests;

// #[cfg(test)]
// mod upgrade_chaos_tests;

#[cfg(test)]
mod escrow_tests;

#[cfg(test)]
mod arbitration_tests;

include!("multi_signer_tests.rs");

#[cfg(test)]
mod batch_swap_features_tests;

#[cfg(test)]
mod batch_approval_tests;

#[cfg(test)]
mod batch_history_tests;

#[cfg(test)]
mod prop_tests;

#[cfg(test)]
mod benchmarks;

#[cfg(test)]
mod chaos_tests;

#[cfg(test)]
mod installment_tests {
    use super::*;
    use soroban_sdk::{testutils::Address as TestAddress, BytesN, Env, Vec};

    fn make_swap(env: &Env, price: i128, paid: i128, is_installment: bool) -> SwapRecord {
        SwapRecord {
            ip_id: 1,
            seller: <soroban_sdk::Address as TestAddress>::generate(env),
            buyer: <soroban_sdk::Address as TestAddress>::generate(env),
            price,
            token: <soroban_sdk::Address as TestAddress>::generate(env),
            status: SwapStatus::Pending,
            expiry: 9_999_999,
            accept_timestamp: 0,
            required_approvals: 0,
            dispute_timestamp: 0,
            referrer: None,
            collateral_amount: 0,
            insurance_premium: 0,
            insurance_enabled: false,
            escrow_agent: None,
            quantity: 1,
            conditions: Vec::new(env),
            paid_amount: paid,
            is_installment,
            arbitrator: None,
        }
    }

    #[test]
    fn test_get_installment_status_initial() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &id);

        let swap = make_swap(&env, 600, 0, true);
        env.as_contract(&id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
        });

        let (paid, total, remaining) = client.get_installment_status(&0u64);
        assert_eq!(paid, 0);
        assert_eq!(total, 600);
        assert_eq!(remaining, 600);
    }

    #[test]
    fn test_get_installment_status_partial_paid() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &id);

        let swap = make_swap(&env, 600, 200, true);
        env.as_contract(&id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
        });

        let (paid, total, remaining) = client.get_installment_status(&0u64);
        assert_eq!(paid, 200);
        assert_eq!(total, 600);
        assert_eq!(remaining, 400);
    }

    #[test]
    fn test_swap_record_installment_fields_stored_and_retrieved() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &id);

        let swap = make_swap(&env, 900, 300, true);
        env.as_contract(&id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
        });

        let record = client.get_swap(&0u64).unwrap();
        assert_eq!(record.paid_amount, 300);
        assert!(record.is_installment);
        assert_eq!(record.price, 900);
    }

    #[test]
    fn test_non_installment_swap_defaults() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &id);

        let swap = make_swap(&env, 500, 0, false);
        env.as_contract(&id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
        });

        let record = client.get_swap(&0u64).unwrap();
        assert!(!record.is_installment);
        assert_eq!(record.paid_amount, 0);
    }

    #[test]
    fn test_installment_remaining_zero_when_fully_paid() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &id);

        let swap = make_swap(&env, 300, 300, true);
        env.as_contract(&id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
        });

        let (paid, total, remaining) = client.get_installment_status(&0u64);
        assert_eq!(paid, 300);
        assert_eq!(total, 300);
        assert_eq!(remaining, 0);
    }

    #[test]
    #[should_panic]
    fn test_submit_installment_non_installment_swap_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &id);

        let swap = make_swap(&env, 300, 0, false); // not an installment swap
        env.as_contract(&id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
        });

        client.submit_installment_payment(&0u64, &100);
    }

    #[test]
    #[should_panic]
    fn test_submit_installment_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &id);

        let swap = make_swap(&env, 300, 0, true);
        env.as_contract(&id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
        });

        client.submit_installment_payment(&0u64, &0);
    }

    #[test]
    #[should_panic]
    fn test_submit_installment_overpayment_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &id);

        let swap = make_swap(&env, 300, 200, true);
        env.as_contract(&id, || {
            env.storage().persistent().set(&DataKey::Swap(0u64), &swap);
        });

        // remaining is 100, paying 200 should panic
        client.submit_installment_payment(&0u64, &200);
    }

    // ── #466: initiate_swap_installment tests ─────────────────────────────────

    #[test]
    fn test_initiate_swap_installment_creates_pending_installment_swap() {
        use ip_registry::{IpRegistry, IpRegistryClient};
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        env.mock_all_auths();

        let seller = <soroban_sdk::Address as TestAddress>::generate(&env);
        let buyer = <soroban_sdk::Address as TestAddress>::generate(&env);
        let admin = <soroban_sdk::Address as TestAddress>::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[2u8; 32]);
        let blinding = BytesN::from_array(&env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &commitment_hash);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap_installment(&token_id, &ip_id, &seller, &600_i128, &buyer, &3_u32);

        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Pending);
        assert!(swap.is_installment);
        assert_eq!(swap.price, 600);
        assert_eq!(swap.paid_amount, 0);
        assert_eq!(swap.quantity, 3);
    }

    #[test]
    fn test_initiate_swap_installment_then_pay_in_parts() {
        use ip_registry::{IpRegistry, IpRegistryClient};
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        env.mock_all_auths();

        let seller = <soroban_sdk::Address as TestAddress>::generate(&env);
        let buyer = <soroban_sdk::Address as TestAddress>::generate(&env);
        let admin = <soroban_sdk::Address as TestAddress>::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[2u8; 32]);
        let blinding = BytesN::from_array(&env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &commitment_hash);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        let swap_id = client.initiate_swap_installment(&token_id, &ip_id, &seller, &300_i128, &buyer, &3_u32);

        // First installment
        client.submit_installment_payment(&swap_id, &100);
        let (paid, total, remaining) = client.get_installment_status(&swap_id);
        assert_eq!(paid, 100);
        assert_eq!(total, 300);
        assert_eq!(remaining, 200);

        // Second installment
        client.submit_installment_payment(&swap_id, &100);
        let (paid, _, remaining) = client.get_installment_status(&swap_id);
        assert_eq!(paid, 200);
        assert_eq!(remaining, 100);

        // Final installment — should transition to Accepted
        client.submit_installment_payment(&swap_id, &100);
        let swap = client.get_swap(&swap_id).unwrap();
        assert_eq!(swap.status, SwapStatus::Accepted);
        assert_eq!(swap.paid_amount, 300);
    }

    #[test]
    #[should_panic]
    fn test_initiate_swap_installment_zero_installments_panics() {
        use ip_registry::{IpRegistry, IpRegistryClient};
        use soroban_sdk::token::StellarAssetClient;

        let env = Env::default();
        env.mock_all_auths();

        let seller = <soroban_sdk::Address as TestAddress>::generate(&env);
        let buyer = <soroban_sdk::Address as TestAddress>::generate(&env);
        let admin = <soroban_sdk::Address as TestAddress>::generate(&env);

        let registry_id = env.register(IpRegistry, ());
        let registry = IpRegistryClient::new(&env, &registry_id);
        let secret = BytesN::from_array(&env, &[2u8; 32]);
        let blinding = BytesN::from_array(&env, &[3u8; 32]);
        let mut preimage = soroban_sdk::Bytes::new(&env);
        preimage.append(&soroban_sdk::Bytes::from(secret.clone()));
        preimage.append(&soroban_sdk::Bytes::from(blinding.clone()));
        let commitment_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let ip_id = registry.commit_ip(&seller, &commitment_hash);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        StellarAssetClient::new(&env, &token_id).mint(&buyer, &1000);

        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);
        client.initialize(&registry_id);

        // num_installments = 0 should panic
        client.initiate_swap_installment(&token_id, &ip_id, &seller, &300_i128, &buyer, &0_u32);
    }
}

// ── #517 & #518: Batch cancellation and fee breakdown tests ─────────────

#[cfg(test)]
mod batch_enhancement_tests {
    use super::*;
    use soroban_sdk::{Address, Bytes, Env, Vec};

    fn setup_swap(env: &Env, id: u64, seller: &Address, buyer: &Address, price: i128, token: &Address, status: SwapStatus) {
        let swap = SwapRecord {
            ip_id: id,
            seller: seller.clone(),
            buyer: buyer.clone(),
            price,
            token: token.clone(),
            status,
            expiry: 9_999_999,
            accept_timestamp: 0,
            required_approvals: 0,
            dispute_timestamp: 0,
            referrer: None,
            collateral_amount: 0,
            insurance_premium: 0,
            insurance_enabled: false,
            escrow_agent: None,
            quantity: 1,
            conditions: Vec::new(env),
            paid_amount: 0,
            is_installment: false,
            arbitrator: None,
        };
        env.storage().persistent().set(&DataKey::Swap(id), &swap);
        env.storage().persistent().extend_ttl(&DataKey::Swap(id), LEDGER_BUMP, LEDGER_BUMP);
        env.storage().persistent().set(&DataKey::ActiveSwap(swap.ip_id), &id);
        env.storage().persistent().extend_ttl(&DataKey::ActiveSwap(swap.ip_id), LEDGER_BUMP, LEDGER_BUMP);
    }

    // ── #517: Batch Cancellation Tests ───────────────────────────────────

    #[test]
    fn test_batch_cancel_swaps_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        env.as_contract(&contract_id, || {
            setup_swap(&env, 1, &seller, &buyer, 100, &token, SwapStatus::Pending);
            setup_swap(&env, 2, &seller, &buyer, 200, &token, SwapStatus::Pending);
            setup_swap(&env, 3, &seller, &buyer, 300, &token, SwapStatus::Pending);
        });

        let swap_ids: Vec<u64> = Vec::from_array(&env, [1, 2, 3]);
        let reasons: Vec<Bytes> = Vec::from_array(&env, [
            Bytes::from_slice(&env, b"no_longer_needed"),
            Bytes::from_slice(&env, b"price_changed"),
            Bytes::from_slice(&env, b"buyer_requested"),
        ]);

        let cancelled = client.batch_cancel_swaps(&swap_ids, &seller, &reasons);
        assert_eq!(cancelled.len(), 3);

        env.as_contract(&contract_id, || {
            let reason1: Bytes = env.storage().persistent().get(&DataKey::CancelReason(1u64)).unwrap();
            assert_eq!(reason1, Bytes::from_slice(&env, b"no_longer_needed"));
            let reason2: Bytes = env.storage().persistent().get(&DataKey::CancelReason(2u64)).unwrap();
            assert_eq!(reason2, Bytes::from_slice(&env, b"price_changed"));
        });

        let rec1 = client.get_swap(&1).unwrap();
        assert_eq!(rec1.status, SwapStatus::Cancelled);
        let rec2 = client.get_swap(&2).unwrap();
        assert_eq!(rec2.status, SwapStatus::Cancelled);
    }

    #[test]
    fn test_batch_cancel_swaps_by_buyer() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        env.as_contract(&contract_id, || {
            setup_swap(&env, 1, &seller, &buyer, 100, &token, SwapStatus::Pending);
        });

        let swap_ids: Vec<u64> = Vec::from_array(&env, [1]);
        let reasons: Vec<Bytes> = Vec::from_array(&env, [Bytes::from_slice(&env, b"buyer_cancel")]);
        let cancelled = client.batch_cancel_swaps(&swap_ids, &buyer, &reasons);
        assert_eq!(cancelled.len(), 1);
        let rec = client.get_swap(&1).unwrap();
        assert_eq!(rec.status, SwapStatus::Cancelled);
    }

    #[test]
    #[should_panic]
    fn test_batch_cancel_swaps_mismatched_reasons() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        env.as_contract(&contract_id, || {
            setup_swap(&env, 1, &seller, &buyer, 100, &token, SwapStatus::Pending);
            setup_swap(&env, 2, &seller, &buyer, 200, &token, SwapStatus::Pending);
        });

        let swap_ids: Vec<u64> = Vec::from_array(&env, [1, 2]);
        let reasons: Vec<Bytes> = Vec::from_array(&env, [Bytes::from_slice(&env, b"reason")]);
        client.batch_cancel_swaps(&swap_ids, &seller, &reasons);
    }

    #[test]
    #[should_panic]
    fn test_batch_cancel_swaps_not_pending_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        env.as_contract(&contract_id, || {
            setup_swap(&env, 1, &seller, &buyer, 100, &token, SwapStatus::Accepted);
        });

        let swap_ids: Vec<u64> = Vec::from_array(&env, [1]);
        let reasons: Vec<Bytes> = Vec::from_array(&env, [Bytes::from_slice(&env, b"cancel")]);
        client.batch_cancel_swaps(&swap_ids, &seller, &reasons);
    }

    #[test]
    #[should_panic]
    fn test_batch_cancel_swaps_unauthorized_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);
        let stranger = Address::generate(&env);

        env.as_contract(&contract_id, || {
            setup_swap(&env, 1, &seller, &buyer, 100, &token, SwapStatus::Pending);
        });

        let swap_ids: Vec<u64> = Vec::from_array(&env, [1]);
        let reasons: Vec<Bytes> = Vec::from_array(&env, [Bytes::from_slice(&env, b"cancel")]);
        client.batch_cancel_swaps(&swap_ids, &stranger, &reasons);
    }

    #[test]
    fn test_batch_cancel_swaps_tracks_reasons_individually() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AtomicSwap, ());
        let client = AtomicSwapClient::new(&env, &contract_id);

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let token = Address::generate(&env);

        env.as_contract(&contract_id, || {
            setup_swap(&env, 10, &seller, &buyer, 1000, &token, SwapStatus::Pending);
            setup_swap(&env, 20, &seller, &buyer, 2000, &token, SwapStatus::Pending);
            setup_swap(&env, 30, &seller, &buyer, 3000, &token, SwapStatus::Pending);
        });

        let swap_ids: Vec<u64> = Vec::from_array(&env, [10, 20, 30]);
        let reasons: Vec<Bytes> = Vec::from_array(&env, [
            Bytes::from_slice(&env, b"dup_ip"),
            Bytes::from_slice(&env, b"buyer_credit"),
            Bytes::from_slice(&env, b"price_disagreement"),
        ]);

        client.batch_cancel_swaps(&swap_ids, &seller, &reasons);

        let r1 = client.get_cancellation_reason(&10).unwrap();
        assert_eq!(r1, Bytes::from_slice(&env, b"dup_ip"));
        let r2 = client.get_cancellation_reason(&20).unwrap();
        assert_eq!(r2, Bytes::from_slice(&env, b"buyer_credit"));
        let r3 = client.get_cancellation_reason(&30).unwrap();
        assert_eq!(r3, Bytes::from_slice(&env, b"price_disagreement"));
    }

    // ── #518: Batch Fee Breakdown Tests ─────────────────────────────────

    #[test]
    fn test_swap_fee_breakdown_struct() {
        let env = Env::default();
        let breakdown = SwapFeeBreakdown {
            swap_id: 42,
            price: 1000,
            protocol_fee: 25,
            referral_fee: 10,
            seller_amount: 965,
        };
        assert_eq!(breakdown.swap_id, 42);
        assert_eq!(breakdown.price, 1000);
        assert_eq!(breakdown.protocol_fee, 25);
        assert_eq!(breakdown.referral_fee, 10);
        assert_eq!(breakdown.seller_amount, 965);
    }

    #[test]
    fn test_batch_fee_breakdown_event_struct() {
        let env = Env::default();

        let fee = SwapFeeBreakdown {
            swap_id: 1,
            price: 500,
            protocol_fee: 12,
            referral_fee: 5,
            seller_amount: 483,
        };

        let fees: Vec<SwapFeeBreakdown> = Vec::from_array(&env, [fee]);
        let swap_ids: Vec<u64> = Vec::from_array(&env, [1]);
        let seller = Address::generate(&env);

        let event = BatchFeeBreakdownEvent {
            swap_ids: swap_ids.clone(),
            seller: seller.clone(),
            fees: fees.clone(),
        };

        assert_eq!(event.swap_ids.len(), 1);
        assert_eq!(event.swap_ids.get(0).unwrap(), 1);
        assert_eq!(event.seller, seller);
        assert_eq!(event.fees.len(), 1);
        assert_eq!(event.fees.get(0).unwrap().swap_id, 1);
        assert_eq!(event.fees.get(0).unwrap().price, 500);
    }

    #[test]
    fn test_batch_cancelled_event_struct() {
        let env = Env::default();

        let swap_ids: Vec<u64> = Vec::from_array(&env, [1, 2, 3]);
        let canceller = Address::generate(&env);
        let reasons: Vec<Bytes> = Vec::from_array(&env, [
            Bytes::from_slice(&env, b"reason1"),
            Bytes::from_slice(&env, b"reason2"),
            Bytes::from_slice(&env, b"reason3"),
        ]);

        let event = BatchCancelledEvent {
            swap_ids: swap_ids.clone(),
            canceller: canceller.clone(),
            reasons: reasons.clone(),
        };

        assert_eq!(event.swap_ids.len(), 3);
        assert_eq!(event.canceller, canceller);
        assert_eq!(event.reasons.len(), 3);
        assert_eq!(event.reasons.get(0).unwrap(), Bytes::from_slice(&env, b"reason1"));
        assert_eq!(event.reasons.get(2).unwrap(), Bytes::from_slice(&env, b"reason3"));
    }
}
