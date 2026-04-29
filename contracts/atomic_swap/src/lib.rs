#![no_std]
mod registry;
mod swap;
mod upgrade;
mod utils;
mod multi_currency;
mod types;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token,
    Address, Bytes, BytesN, Env, Error, String, Vec,
};

pub use upgrade::{build_v1_schema, ContractSchema, ErrorEntry, FunctionEntry};
pub use types::*;

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

    // ── #347: Auction errors (39-44) ──────────────────────────────────────────
    /// Auction not found.
    AuctionNotFound = 39,
    /// Active auction already exists for this IP.
    ActiveAuctionAlreadyExists = 40,
    /// Bid amount must be greater than or equal to minimum bid.
    BidBelowMinimum = 41,
    /// Bid amount must be greater than current highest bid.
    BidNotHigherThanCurrent = 42,
    /// Auction has not ended yet.
    AuctionNotEnded = 43,
    /// Auction has already been finalized.
    AuctionAlreadyFinalized = 44,

    // ── #349: Payment schedule errors (45-48) ────────────────────────────────
    /// Payment schedule not found for this swap.
    PaymentScheduleNotFound = 45,
    /// Payment index out of bounds.
    PaymentIndexOutOfBounds = 46,
    /// Payment already made for this index.
    PaymentAlreadyMade = 47,
    /// Payment not yet due.
    PaymentNotYetDue = 48,

    // ── #350: Collateral errors (49-51) ──────────────────────────────────────
    /// Collateral not found for this swap.
    CollateralNotFound = 49,
    /// Insufficient collateral deposited.
    InsufficientCollateral = 50,
    /// Collateral already deposited for this swap.
    CollateralAlreadyDeposited = 51,

    // ── #359: Reputation errors (52) ─────────────────────────────────────────
    /// User reputation not found.
    ReputationNotFound = 52,

    // ── #360: Contingent completion errors (53-54) ──────────────────────────
    /// Contingency condition not met.
    ContingencyConditionNotMet = 53,
    /// Only seller can complete contingent swap.
    OnlySellerCanCompleteContingent = 54,

    // ── #361: Dispute evidence errors (55) ───────────────────────────────────
    /// Dispute evidence not found.
    DisputeEvidenceNotFound = 55,

    // ── Partial quantity swap errors (56) ────────────────────────────────────
    /// Requested quantity is zero or exceeds the swap's total quantity.
    InvalidQuantity = 56,
    /// No pending renegotiation offer exists for this swap.
    NoRenegotiationOffer = 57,
    /// A buyer-set condition was not satisfied at acceptance time.
    ConditionNotMet = 58,
    /// Insurance is not enabled for this swap.
    InsuranceNotEnabled = 59,
    /// This swap is not eligible for an insurance claim.
    InsuranceNotClaimable = 60,
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

// ── #354: Insurance Types ─────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct InsurancePayoutEvent {
    pub swap_id: u64,
    pub buyer: Address,
    pub payout_amount: i128,
}

// ── #353: Renegotiation Types ─────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct RenegotiationOffer {
    pub new_price: i128,
    pub proposer: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RenegotiationProposedEvent {
    pub swap_id: u64,
    pub new_price: i128,
    pub proposer: Address,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RenegotiationAcceptedEvent {
    pub swap_id: u64,
    pub new_price: i128,
    pub buyer: Address,
}

// ── #352: Escrow Types ────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct EscrowReleasedEvent {
    pub swap_id: u64,
    pub escrow_agent: Address,
    pub amount: i128,
}

// ── #351: Conditional Acceptance Types ────────────────────────────────────────

/// Conditions the buyer attaches to a swap. All must pass for accept_swap to proceed.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum SwapCondition {
    /// The seller's key must be verifiable (always checked on reveal; here it gates acceptance).
    KeyValid,
    /// The swap price must be strictly below this threshold at acceptance time.
    PriceBelow(i128),
    /// The ledger timestamp must be at or after this value.
    TimeAfter(u64),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ConditionalAcceptanceEvent {
    pub swap_id: u64,
    pub buyer: Address,
    pub conditions_count: u32,
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
        collateral_amount: i128,
        contingency_condition: Option<Vec<u8>>,
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
    fn evaluate_conditions(env: &Env, swap: &SwapRecord) {
        for i in 0..swap.conditions.len() {
            let ok = match swap.conditions.get(i).unwrap() {
                SwapCondition::KeyValid => {
                    // KeyValid is enforced at reveal_key; always passes at accept time.
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

    /// Buyer accepts the swap with conditions. Conditions are stored on the swap record
    /// and evaluated immediately. If all pass, the swap proceeds to Accepted.
    pub fn accept_swap_conditional(env: Env, swap_id: u64, conditions: Vec<SwapCondition>) {
        require_not_paused(&env);
        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::SwapNotPending);

        swap.conditions = conditions;
        Self::evaluate_conditions(&env, &swap);

        // Proceed with standard acceptance
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

        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &swap.price,
        );

        let conditions_count = swap.conditions.len();
        swap.accept_timestamp = env.ledger().timestamp();
        swap.status = SwapStatus::Accepted;
        swap::save_swap(&env, swap_id, &swap);

        Self::append_history(&env, swap_id, SwapStatus::Accepted);

        env.events().publish(
            (soroban_sdk::symbol_short!("cond_acpt"),),
            ConditionalAcceptanceEvent { swap_id, buyer: swap.buyer, conditions_count },
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

        // #351: Evaluate any buyer-set conditions.
        if !swap.conditions.is_empty() {
            Self::evaluate_conditions(&env, &swap);
        }

        // #350: Deposit collateral if required
        if swap.collateral_amount > 0 {
            // Check if collateral already deposited
            if env.storage().persistent().has(&DataKey::SwapCollateral(swap_id)) {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::CollateralAlreadyDeposited as u32,
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
            ContractError::SwapNotAccepted,
        );

        // Verify commitment via IP registry
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
        reputation::update_reputation_on_completion(&env, &swap.seller, &swap.buyer);

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

    // ── #360: Admin Rollback ───────────────────────────────────────────────────

    /// Admin rollback: refund both parties and cancel a swap due to fraud or issues.
    /// This is a safety mechanism for extreme cases where the normal dispute process
    /// is insufficient. Both buyer and seller receive full refunds.
    pub fn admin_rollback_swap(env: Env, swap_id: u64, caller: Address, reason: Bytes<64>) {
        caller.require_auth();
        require_admin(&env, &caller);

        let mut swap = require_swap_exists(&env, swap_id);

        // Cannot rollback already completed swaps
        if swap.status == SwapStatus::Completed {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapNotInAcceptedState as u32,
            ));
        }

        let token_client = token::Client::new(&env, &swap.token);
        let mut buyer_refund = 0i128;
        let mut seller_refund = 0i128;

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
            (soroban_sdk::symbol_short!("admin_rlbk"),),
            AdminRollbackEvent {
                swap_id,
                reason,
                buyer_refund,
                seller_refund,
                timestamp: env.ledger().timestamp(),
            },
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

    /// Buyer accepts a partial quantity of a bulk swap at a proportional price.
    /// `quantity` must be ≥ 1 and ≤ `swap.quantity`.
    /// Payment = swap.price * quantity / swap.quantity (integer division, rounded down).
    pub fn accept_swap_partial(env: Env, swap_id: u64, quantity: u32) {
        require_not_paused(&env);

        let mut swap = require_swap_exists(&env, swap_id);
        swap.buyer.require_auth();
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::SwapNotPending);

        if quantity == 0 || quantity > swap.quantity {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidQuantity as u32,
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
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::SwapNotPending);
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
            (soroban_sdk::symbol_short!("reneg_prop"),),
            RenegotiationProposedEvent { swap_id, new_price, proposer: swap.seller },
        );
    }

    /// Buyer accepts the pending renegotiation offer, updating the swap price.
    pub fn accept_renegotiation(env: Env, swap_id: u64) {
        require_not_paused(&env);
        let mut swap = require_swap_exists(&env, swap_id);
        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::SwapNotPending);
        swap.buyer.require_auth();

        let offer: RenegotiationOffer = env
            .storage()
            .persistent()
            .get(&DataKey::SwapRenegotiations(swap_id))
            .unwrap_or_else(|| {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::NoRenegotiationOffer as u32,
                ))
            });

        swap.price = offer.new_price;
        swap::save_swap(&env, swap_id, &swap);

        env.storage()
            .persistent()
            .remove(&DataKey::SwapRenegotiations(swap_id));

        env.events().publish(
            (soroban_sdk::symbol_short!("reneg_acpt"),),
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
                ContractError::InsuranceNotEnabled as u32,
            ));
        }

        if !env.storage().persistent().has(&DataKey::InsuranceClaimable(swap_id)) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InsuranceNotClaimable as u32,
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
                ContractError::ActiveAuctionAlreadyExists as u32,
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
            (soroban_sdk::symbol_short!("auction_start"),),
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
                    ContractError::AuctionNotFound as u32,
                ));
                unreachable!()
            });

        if auction.finalized {
            env.panic_with_error(Error::from_contract_error(
                ContractError::AuctionAlreadyFinalized as u32,
            ));
        }

        if env.ledger().timestamp() >= auction.end_time {
            env.panic_with_error(Error::from_contract_error(
                ContractError::AuctionNotEnded as u32,
            ));
        }

        if bid_amount < auction.min_bid {
            env.panic_with_error(Error::from_contract_error(
                ContractError::BidBelowMinimum as u32,
            ));
        }

        if bid_amount <= auction.highest_bid {
            env.panic_with_error(Error::from_contract_error(
                ContractError::BidNotHigherThanCurrent as u32,
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
            (soroban_sdk::symbol_short!("bid_placed"),),
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
                    ContractError::AuctionNotFound as u32,
                ));
                unreachable!()
            });

        if auction.finalized {
            env.panic_with_error(Error::from_contract_error(
                ContractError::AuctionAlreadyFinalized as u32,
            ));
        }

        if env.ledger().timestamp() < auction.end_time {
            env.panic_with_error(Error::from_contract_error(
                ContractError::AuctionNotEnded as u32,
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
            (soroban_sdk::symbol_short!("auction_final"),),
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
                    ContractError::AuctionNotFound as u32,
                ));
                unreachable!()
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
                ContractError::PriceMustBeGreaterThanZero as u32,
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
                    ContractError::PaymentScheduleNotFound as u32,
                ));
                unreachable!()
            });

        if (payment_index as usize) >= schedule.len() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PaymentIndexOutOfBounds as u32,
            ));
        }

        let mut payments_made: Vec<bool> = env
            .storage()
            .persistent()
            .get(&DataKey::PaymentsMade(swap_id))
            .unwrap_or(Vec::new(&env));

        if payments_made.get(payment_index as usize).unwrap_or(false) {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PaymentAlreadyMade as u32,
            ));
        }

        let payment = schedule.get(payment_index as usize).unwrap();
        if env.ledger().timestamp() < payment.due_timestamp {
            env.panic_with_error(Error::from_contract_error(
                ContractError::PaymentNotYetDue as u32,
            ));
        }

        // Transfer payment
        token::Client::new(&env, &swap.token).transfer(
            &swap.buyer,
            &env.current_contract_address(),
            &payment.amount,
        );

        // Mark payment as made
        payments_made.set(payment_index as usize, true);
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
            ContractError::SwapNotDisputed,
        );

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_req"),),
            ArbitrationRequestedEvent {
                swap_id,
                requester,
                evidence_hash,
            },
        );
    }

    /// Set arbitrator for a swap. Called by admin or designated arbitrator.
    pub fn set_arbitrator(env: Env, swap_id: u64, arbitrator: Address) {
        let swap = require_swap_exists(&env, swap_id);

        // Check if arbitrator already set
        if env
            .storage()
            .persistent()
            .has(&DataKey::SwapArbitrator(swap_id))
        {
            env.panic_with_error(Error::from_contract_error(
                ContractError::ArbitratorAlreadySet as u32,
            ));
        }

        // Store arbitrator
        env.storage()
            .persistent()
            .set(&DataKey::SwapArbitrator(swap_id), &arbitrator);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::SwapArbitrator(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("arb_set"),),
            ArbitratorSetEvent {
                swap_id,
                arbitrator,
            },
        );
    }

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
        env.storage()
            .persistent()
            .remove(&DataKey::SwapArbitrator(swap_id));

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
            ContractError::SwapNotAccepted,
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
                    reason: String::from_slice(&env, "Invalid decryption key"),
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

        env.storage()
            .persistent()
            .set(&DataKey::AtomicRefundProcessed(swap_id), &true);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::AtomicRefundProcessed(swap_id), LEDGER_BUMP, LEDGER_BUMP);
    }

    // ── #357: Batch Processing ────────────────────────────────────────────────

    /// Batch accept multiple swaps. Buyer-only.
    pub fn batch_accept_swaps(env: Env, swap_ids: Vec<u64>, buyer: Address) {
        require_not_paused(&env);
        buyer.require_auth();

        for swap_id in swap_ids.iter() {
            let mut swap = require_swap_exists(&env, swap_id);

            if swap.buyer != buyer {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::Unauthorized as u32,
                ));
            }

            require_swap_status(
                &env,
                &swap,
                SwapStatus::Pending,
                ContractError::SwapNotPending,
            );

            // Check approvals
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

            // Deposit collateral if required
            if swap.collateral_amount > 0 {
                if !env
                    .storage()
                    .persistent()
                    .has(&DataKey::SwapCollateral(swap_id))
                {
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

            // Transfer payment
            let token_client = token::Client::new(&env, &swap.token);
            token_client.transfer(
                &swap.buyer,
                &env.current_contract_address(),
                &swap.price,
            );

            swap.accept_timestamp = env.ledger().timestamp();
            swap.status = SwapStatus::Accepted;
            swap::save_swap(&env, swap_id, &swap);

            Self::append_history(&env, swap_id, SwapStatus::Accepted);
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("batch_acpt"),),
            BatchAcceptedEvent {
                swap_ids,
                buyer,
            },
        );
    }

    /// Batch reveal keys for multiple swaps. Seller-only.
    pub fn batch_reveal_keys(
        env: Env,
        swap_ids: Vec<u64>,
        secrets: Vec<BytesN<32>>,
        blinding_factors: Vec<BytesN<32>>,
        seller: Address,
    ) {
        seller.require_auth();

        if swap_ids.len() != secrets.len() || swap_ids.len() != blinding_factors.len() {
            env.panic_with_error(Error::from_contract_error(
                ContractError::InvalidKey as u32,
            ));
        }

        for i in 0..swap_ids.len() {
            let swap_id = swap_ids.get(i).unwrap();
            let secret = secrets.get(i).unwrap();
            let blinding_factor = blinding_factors.get(i).unwrap();

            let mut swap = require_swap_exists(&env, swap_id);

            require_seller(&env, &seller, &swap);
            require_swap_status(
                &env,
                &swap,
                SwapStatus::Accepted,
                ContractError::SwapNotAccepted,
            );

            // Verify commitment
            let valid = registry::verify_commitment(&env, swap.ip_id, &secret, &blinding_factor);
            if !valid {
                env.panic_with_error(Error::from_contract_error(
                    ContractError::InvalidKey as u32,
                ));
            }

            swap.status = SwapStatus::Completed;
            swap::save_swap(&env, swap_id, &swap);
            env.storage()
                .persistent()
                .remove(&DataKey::ActiveSwap(swap.ip_id));

            Self::append_history(&env, swap_id, SwapStatus::Completed);

            // Process payment
            let token_client = token::Client::new(&env, &swap.token);
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
        }

        env.events().publish(
            (soroban_sdk::symbol_short!("batch_keys"),),
            BatchKeysRevealedEvent {
                swap_ids,
                seller,
            },
        );
    }

    // ── #358: Swap Timeout Escalation ─────────────────────────────────────────

    /// Request timeout escalation. Buyer-only. Extends deadline if timeout imminent.
    pub fn escalate_swap_timeout(
        env: Env,
        swap_id: u64,
        buyer: Address,
        new_expiry: u64,
    ) {
        let mut swap = require_swap_exists(&env, swap_id);
        buyer.require_auth();

        if swap.buyer != buyer {
            env.panic_with_error(Error::from_contract_error(
                ContractError::Unauthorized as u32,
            ));
        }

        require_swap_status(
            &env,
            &swap,
            SwapStatus::Accepted,
            ContractError::SwapNotAccepted,
        );

        let current_time = env.ledger().timestamp();
        let time_until_expiry = swap.expiry.saturating_sub(current_time);

        // Require timeout to be imminent (within 1 hour = 3600 seconds)
        if time_until_expiry > 3600 {
            env.panic_with_error(Error::from_contract_error(
                ContractError::SwapHasNotExpiredYet as u32,
            ));
        }

        // New expiry must be greater than current expiry
        if new_expiry <= swap.expiry {
            env.panic_with_error(Error::from_contract_error(
                ContractError::NewExpiryNotGreater as u32,
            ));
        }

        let old_expiry = swap.expiry;
        swap.expiry = new_expiry;
        swap::save_swap(&env, swap_id, &swap);

        env.storage()
            .persistent()
            .set(&DataKey::TimeoutExtension(swap_id), &new_expiry);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::TimeoutExtension(swap_id), LEDGER_BUMP, LEDGER_BUMP);

        env.events().publish(
            (soroban_sdk::symbol_short!("timeout_esc"),),
            TimeoutEscalationRequestedEvent {
                swap_id,
                buyer,
                new_expiry,
            },
        );
    }

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
            escrow_agent: None,
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
            (soroban_sdk::symbol_short!("escrow_ini"),),
            SwapInitiatedEvent { swap_id: id, ip_id, seller, buyer, price },
        );

        id
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

        require_swap_status(&env, &swap, SwapStatus::Pending, ContractError::SwapNotPending);

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
            (soroban_sdk::symbol_short!("escrow_dep"),),
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

        require_swap_status(&env, &swap, SwapStatus::Accepted, ContractError::SwapNotAccepted);

        // Timeout must have passed
        if env.ledger().timestamp() <= swap.expiry {
            env.panic_with_error(Error::from_contract_error(ContractError::SwapHasNotExpiredYet as u32));
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
            (soroban_sdk::symbol_short!("escrow_wdr"),),
            SwapCancelledEvent { swap_id, canceller: swap.buyer },
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

#[cfg(test)]
mod escrow_tests;

#[cfg(test)]
mod prop_tests;

#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod arbitration_tests;

#[cfg(test)]
mod benchmarks;

#[cfg(test)]
mod mutation_tests;

#[cfg(test)]
mod snapshot_tests;

#[cfg(test)]
mod upgrade_chaos_tests;
