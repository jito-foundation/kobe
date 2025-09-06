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
    pub vote_account: String,
    pub mev_commission_bps: Option<u16>,
    pub mev_rewards: Option<u64>,
    pub priority_fee_commission_bps: Option<u16>,
    pub priority_fee_rewards: Option<u64>,
    pub running_jito: bool,
    pub active_stake: u64,

    /// Is Jito Blacklist
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
