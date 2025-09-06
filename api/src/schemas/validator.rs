use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct JitoStakeOverTimeResponse {
    pub stake_ratio_over_time: HashMap<u64, f64>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct AverageMevCommissionOverTimeResponse {
    pub average_mev_commission_over_time: HashMap<u64, f64>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct ValidatorsResponse {
    pub validators: Vec<ValidatorEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ValidatorEntry {
    /// Vote account public key
    pub vote_account: String,

    /// MEV commission rate in basis points (100 bps = 1%)
    pub mev_commission_bps: Option<u16>,

    /// MEV rewards earned for this specific epoch in lamports
    pub mev_rewards: Option<u64>,

    /// Priority fee commission rate in basis points (100 bps = 1%)
    pub priority_fee_commission_bps: Option<u16>,

    /// Priority fee rewards earned for this specific epoch in lamports
    pub priority_fee_rewards: Option<u64>,

    /// Whether this validator is running the Jito client
    pub running_jito: bool,

    /// Amount of actively staked SOL in lamports
    pub active_stake: u64,

    /// Whether this is blacklisted by Jito
    pub is_jito_blacklist: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ValidatorByVoteAccountResponse {
    pub epoch: u64,
    pub mev_commission_bps: u16,
    pub mev_rewards: u64,
    pub priority_fee_commission_bps: u16,
    pub priority_fee_rewards: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ValidatorsRequest {
    pub epoch: u64,
}

impl std::fmt::Display for ValidatorsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.epoch)
    }
}
