/// Canonical error-code reference for the AtomicSwap contract.
///
/// This file mirrors the `ContractError` enum defined in `lib.rs` (which is
/// the authoritative definition used by the Soroban `#[contracterror]` macro).
/// It exists as a human-readable reference and is kept in sync manually.
///
/// # Upgrade safety
///
/// Numeric discriminants MUST NOT change across contract upgrades.  Off-chain
/// clients and indexers rely on stable codes to identify error conditions.
#[repr(u32)]
pub enum ContractError {
    SwapNotFound                          = 1,
    InvalidKey                            = 2,
    PriceMustBeGreaterThanZero            = 3,
    SellerIsNotTheIPOwner                 = 4,
    ActiveSwapAlreadyExistsForThisIpId    = 5,
    SwapNotPending                        = 6,
    OnlyTheSellerCanRevealTheKey          = 7,
    SwapNotAccepted                       = 8,
    OnlyTheSellerOrBuyerCanCancel         = 9,
    OnlyPendingSwapsCanBeCancelledThisWay = 10,
    SwapNotInAcceptedState                = 11,
    OnlyTheBuyerCanCancelAnExpiredSwap    = 12,
    SwapHasNotExpiredYet                  = 13,
    IpIsRevoked                           = 14,
    UnauthorizedUpgrade                   = 15,
    InvalidFeeBps                         = 16,
    DisputeWindowExpired                  = 17,
    OnlyBuyerCanDispute                   = 18,
    SwapNotDisputed                       = 19,
    OnlyAdminCanResolve                   = 20,
    ContractPaused                        = 21,
    AlreadyInitialized                    = 22,
    Unauthorized                          = 23,
    NotInitialized                        = 24,
    PendingSwapNotExpired                 = 25,
    NewExpiryNotGreater                   = 26,
    InsufficientApprovals                 = 27,
    AlreadyApproved                       = 28,
    // Upgrade-validation errors
    UpgradeSchemaVersionNotGreater        = 29,
    UpgradeMissingFunction                = 30,
    UpgradeFunctionSignatureChanged       = 31,
    UpgradeMissingErrorCode               = 32,
    UpgradeErrorCodeChanged               = 33,
    UpgradeMissingStorageKey              = 34,
}
