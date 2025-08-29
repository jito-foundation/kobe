use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct MevRewardsRequest {
    pub epoch: u64,
}

impl std::fmt::Display for MevRewardsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.epoch)
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct MevRewards {
    /// Epoch in question
    pub epoch: u64,
    /// Total post-Labs fee MEV distributed to validators + stakers for that epoch
    pub total_network_mev_lamports: u64,
    /// Total Jito stake, in lamports
    pub jito_stake_weight_lamports: u64,
    /// total_network_mev_lamports divided by jito_stake_weight_lamports.
    pub mev_reward_per_lamport: f64,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct ValidatorRewardsRequest {
    pub vote_account: Option<String>,
    pub epoch: Option<u64>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub sort_order: Option<String>,
}

impl std::fmt::Display for ValidatorRewardsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,
            "ValidatorRewardsRequest {{ vote_account: {:?}, epoch: {:?}, page: {:?}, limit: {:?}, sort_order: {:?} }}",
            self.vote_account, self.epoch, self.page, self.limit, self.sort_order
        )
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct ValidatorRewards {
    pub vote_account: String,
    pub mev_revenue: u64,
    pub mev_commission: u16,
    pub priority_fee_revenue: u64,
    pub priority_fee_commission: Option<u16>,
    pub num_stakers: u64,
    pub epoch: u64,
    pub claim_status_account: Option<String>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct ValidatorRewardsResponse {
    pub rewards: Vec<ValidatorRewards>,
    pub total_count: u64,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct StakerRewardsRequest {
    pub stake_authority: Option<String>,
    pub validator_vote_account: Option<String>,
    pub epoch: Option<u64>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub sort_order: Option<String>,
}

impl std::fmt::Display for StakerRewardsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StakerRewardsRequest {{ stake_authority: {:?}, validator_vote_account: {:?}, epoch: {:?}, page: {:?}, limit: {:?}, sort_order: {:?} }}", self.stake_authority, self.validator_vote_account, self.epoch, self.page, self.limit, self.sort_order)
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct StakerRewards {
    pub claimant: String,
    pub stake_authority: String,
    pub withdraw_authority: String,
    pub validator_vote_account: String,
    pub epoch: u64,
    pub amount: u64,
    pub claim_status_account: Option<String>,
    pub priority_fee_amount: Option<u64>,
    pub priority_fee_claim_status_account: Option<String>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct StakerRewardsResponse {
    pub rewards: Vec<StakerRewards>,
    pub total_count: u64,
}
