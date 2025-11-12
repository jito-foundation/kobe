use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
    /// Identity account pubkey
    pub identity_account: Option<String>,

    /// Vote account pubkey
    pub vote_account: String,
    pub mev_commission_bps: Option<u16>,
    pub mev_rewards: Option<u64>,
    pub priority_fee_commission_bps: Option<u16>,
    pub priority_fee_rewards: Option<u64>,
    /// Whether or not running Jito client
    pub running_jito: bool,

    /// Whether or not running BAM client
    pub running_bam: Option<bool>,

    /// Total active stake delegated to this validator on the Solana network
    pub active_stake: u64,

    /// Active stake lamports delegated to this validator from the JitoSOL stake-pool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jito_sol_active_lamports: Option<u64>,

    /// Whether or not jito pool eligible validator
    pub jito_pool_eligible: Option<bool>,

    /// Indicates whether this validator is a target for directed stake from the JitoSOL stake-pool.
    pub jito_pool_directed_stake_target: Option<bool>,
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
