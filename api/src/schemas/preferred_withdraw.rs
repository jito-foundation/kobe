use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct PreferredWithdraw {
    /// Index in the validator list
    pub index: u16,

    /// Validator vote account address
    pub vote_account: String,

    /// Amount we can withdraw (respecting minimum thresholds)
    pub withdrawable_lamports: u64,

    /// Validator score (lower = worse performing)
    pub score: u64,

    /// Validator stake account address
    pub stake_account: String,
}
