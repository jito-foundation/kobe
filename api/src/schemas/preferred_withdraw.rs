use serde::{Deserialize, Serialize};

/// Request parameters for the preferred withdraw validator list endpoint
#[derive(Debug, Deserialize, Clone)]
pub struct PreferredWithdrawRequest {
    /// Minimum stake threshold in lamports (defaults to 10_000 SOL)
    pub min_stake_threshold: Option<u64>,

    /// Number of validators to return (defaults to 50)
    pub list_size: Option<u32>,

    /// Whether to randomize the list order (defaults to false)
    pub randomized: Option<bool>,
}

impl std::fmt::Display for PreferredWithdrawRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PreferredWithdrawRequest {{ min_stake_threshold: {:?}, list_size: {:?}, randomized: {:?} }}",
            self.min_stake_threshold,
            self.list_size,
            self.randomized
        )
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct PreferredWithdraw {
    /// Index in the validator list
    pub rank: u16,

    /// Validator vote account address
    pub vote_account: String,

    /// Amount we can withdraw (respecting minimum thresholds)
    pub withdrawable_lamports: u64,

    /// Validator stake account address
    pub stake_account: String,
}
