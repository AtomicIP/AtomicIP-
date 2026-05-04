use soroban_sdk::{contracttype, Address, BytesN, Vec};

// ── TTL ───────────────────────────────────────────────────────────────────────

/// Minimum ledger TTL bump applied to every persistent storage write.
/// ~1 year at ~5s per ledger: 365 * 24 * 3600 / 5 ≈ 6_307_200 ledgers.
#[allow(dead_code)]
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
    /// Maps ip_id → Vec<u64> of all swap IDs ever created for that IP.
    IpSwaps(u64),
    /// Whether the contract is paused (blocks initiate_swap and accept_swap).
    Paused,
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
    /// #355: Maps swap_id → arbitrator Address for dispute resolution.
    SwapArbitrator(u64),
    /// #356: Maps swap_id → bool indicating if atomic refund was processed.
    AtomicRefundProcessed(u64),
    /// #358: Maps swap_id → new expiry timestamp for timeout escalation.
    TimeoutExtension(u64),
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

// SwapRecord is defined in lib.rs (not a contracttype due to Vec<SwapCondition> field)

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

// ProtocolConfig is defined in lib.rs (needs to be in same file as contractimpl)

// ── #311: Referral Paid Event ─────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ReferralPaidEvent {
    pub swap_id: u64,
    pub referrer: Address,
    pub referral_amount: i128,
}

// ── #253: Swap History ────────────────────────────────────────────────────────

/// A single state-transition entry in the swap audit trail.
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

// ── #360: Admin Rollback Event ───────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AdminRollbackEvent {
    pub swap_id: u64,
    pub reason: soroban_sdk::Bytes,
    pub buyer_refund: i128,
    pub seller_refund: i128,
    pub timestamp: u64,
}

// ── #313: Dispute Evidence Event ──────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DisputeEvidenceSubmittedEvent {
    pub swap_id: u64,
    pub submitter: Address,
    pub evidence_hash: BytesN<32>,
}

// ── #347: Auction Types ───────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct AuctionRecord {
    pub auction_id: u64,
    pub ip_id: u64,
    pub seller: Address,
    pub token: Address,
    pub min_bid: i128,
    pub highest_bid: i128,
    pub highest_bidder: Option<Address>,
    pub start_time: u64,
    pub end_time: u64,
    pub finalized: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AuctionStartedEvent {
    pub auction_id: u64,
    pub ip_id: u64,
    pub seller: Address,
    pub min_bid: i128,
    pub end_time: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BidPlacedEvent {
    pub auction_id: u64,
    pub bidder: Address,
    pub bid_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AuctionFinalizedEvent {
    pub auction_id: u64,
    pub winner: Option<Address>,
    pub winning_bid: i128,
}

// ── #349: Payment Schedule Types ──────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct PaymentSchedule {
    pub due_timestamp: u64,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledPaymentMadeEvent {
    pub swap_id: u64,
    pub payment_index: u32,
    pub amount: i128,
    pub remaining_payments: u32,
}

// ── #350: Collateral Types ────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CollateralDepositedEvent {
    pub swap_id: u64,
    pub buyer: Address,
    pub collateral_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CollateralReleasedEvent {
    pub swap_id: u64,
    pub buyer: Address,
    pub collateral_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CollateralRefundedEvent {
    pub swap_id: u64,
    pub buyer: Address,
    pub collateral_amount: i128,
}


// ── #355: Arbitration Request Event ───────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ArbitrationRequestedEvent {
    pub swap_id: u64,
    pub requester: Address,
    pub evidence_hash: BytesN<32>,
}

// ── #356: Atomic Refund Event ─────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AtomicRefundEvent {
    pub swap_id: u64,
    pub buyer: Address,
    pub refund_amount: i128,
    pub reason: soroban_sdk::String,
}

// ── #357: Batch Processing Events ─────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchAcceptedEvent {
    pub swap_ids: Vec<u64>,
    pub buyer: Address,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchKeysRevealedEvent {
    pub swap_ids: Vec<u64>,
    pub seller: Address,
}

// ── #358: Timeout Escalation Event ────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TimeoutEscalationRequestedEvent {
    pub swap_id: u64,
    pub buyer: Address,
    pub new_expiry: u64,
}

// ── #352: Renegotiation Types ─────────────────────────────────────────────────

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

// ── #354: Insurance Types ─────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct InsurancePayoutEvent {
    pub swap_id: u64,
    pub buyer: Address,
    pub payout_amount: i128,
}
