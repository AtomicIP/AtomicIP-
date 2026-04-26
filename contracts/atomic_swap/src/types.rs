use soroban_sdk::{contracttype, Address, BytesN, Vec};

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
    /// #359: Maps Address → (completed_swaps: u32, rating: u32) for user reputation.
    UserReputation(Address),
    /// #360: Maps swap_id → contingency_condition: Bytes for conditional completion.
    SwapContingency(u64),
    /// #361: Maps swap_id → Vec<Bytes> of evidence hashes for disputes.
    SwapDisputeEvidence(u64),
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
    /// #350: Collateral amount required from buyer. Zero if no collateral.
    pub collateral_amount: i128,
    /// #360: Optional contingency condition for delayed finalization.
    pub contingency_condition: Option<Vec<u8>>,
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
#[derive(Clone)]
pub struct ProtocolConfig {
    pub protocol_fee_bps: u32,  // 0-10000 (0.00% - 100.00%)
    pub treasury: Address,
    pub dispute_window_seconds: u64,
    pub dispute_resolution_timeout_seconds: u64,
    /// #311: Referral fee in basis points (0-10000). Deducted from seller proceeds.
    pub referral_fee_bps: u32,
}

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

// ── #359: User Reputation Types ───────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UserReputation {
    pub completed_swaps: u32,
    pub rating: u32,
}

// ── #360: Contingent Completion Types ─────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SwapContingentCompletedEvent {
    pub swap_id: u64,
    pub seller: Address,
}

// ── #361: Dispute Evidence Types ──────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DisputeEvidenceStoredEvent {
    pub swap_id: u64,
    pub submitter: Address,
    pub evidence_hash: BytesN<32>,
}
