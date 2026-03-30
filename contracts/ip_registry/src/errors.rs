#[repr(u32)]
pub enum ContractError {
    IpNotFound = 1,
    ZeroCommitmentHash = 2,
    CommitmentAlreadyRegistered = 3,
    IpAlreadyRevoked = 4,
    UnauthorizedUpgrade = 5,
}
