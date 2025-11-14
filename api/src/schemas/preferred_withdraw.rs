use serde::{Deserialize, Serialize};

/// Request parameters for the preferred withdraw validator list endpoint
#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct PreferredWithdrawRequest {
    /// Minimum stake threshold (denominated in SOL)
    pub min_stake_threshold: u64,

    /// Number of validators to return
    pub limit: u32,

    /// Whether to randomize the list order
    pub randomized: bool,
}

impl Default for PreferredWithdrawRequest {
    fn default() -> Self {
        Self {
            // Denominated in SOL
            min_stake_threshold: 10_000,
            limit: 50,
            randomized: false,
        }
    }
}

impl std::fmt::Display for PreferredWithdrawRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PreferredWithdrawRequest {{ min_stake_threshold: {:?}, limit: {:?}, randomized: {:?} }}",
            self.min_stake_threshold,
            self.limit,
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
