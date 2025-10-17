use serde::{Deserialize, Serialize};
use validator_history::{ClientVersion, ValidatorHistory, ValidatorHistoryEntry};

#[derive(Deserialize)]
pub struct EpochQuery {
    pub epoch: Option<u16>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct ValidatorHistoryResponse {
    /// Cannot be enum due to Pod and Zeroable trait limitations
    pub struct_version: u32,

    /// Vote account address
    pub vote_account: String,

    /// Index of validator of all ValidatorHistory accounts
    pub index: u32,

    /// These Crds gossip values are only signed and dated once upon startup and then never updated
    /// so we track latest time on-chain to make sure old messages aren't uploaded
    pub(crate) last_ip_timestamp: u64,

    pub(crate) last_version_timestamp: u64,

    pub(crate) history: Vec<ValidatorHistoryEntryResponse>,
}

impl ValidatorHistoryResponse {
    pub fn from_validator_history(
        acc: ValidatorHistory,
        history_entries: Vec<ValidatorHistoryEntryResponse>,
    ) -> Self {
        Self {
            struct_version: acc.struct_version,
            vote_account: acc.vote_account.to_string(),
            index: acc.index,
            last_ip_timestamp: acc.last_ip_timestamp,
            last_version_timestamp: acc.last_version_timestamp,
            history: history_entries,
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct ValidatorHistoryEntryResponse {
    /// Activated stake lamports
    pub activated_stake_lamports: u64,

    /// Epoch number
    pub epoch: u16,

    // MEV commission in basis points
    pub mev_commission: u16,

    // Number of successful votes in current epoch. Not finalized until subsequent epoch
    pub epoch_credits: u32,

    // Validator commission in points
    pub commission: u8,

    // 0 if Solana Labs client, 1 if Jito client, >1 if other
    pub client_type: u8,

    pub version: ClientVersionResponse,

    pub ip: [u8; 4],

    // 0 if not a superminority validator, 1 if superminority validator
    pub is_superminority: u8,

    // rank of validator by stake amount
    pub rank: u32,

    // Most recent updated slot for epoch credits and commission
    pub vote_account_last_update_slot: u64,

    // MEV earned, stored as 1/100th SOL. mev_earned = 100 means 1.00 SOL earned
    pub mev_earned: u32,
}

impl ValidatorHistoryEntryResponse {
    pub fn from_validator_history_entry(entry: &ValidatorHistoryEntry) -> Self {
        let version = ClientVersionResponse::from_client_version(entry.version);
        Self {
            activated_stake_lamports: entry.activated_stake_lamports,
            epoch: entry.epoch,
            mev_commission: entry.mev_commission,
            epoch_credits: entry.epoch_credits,
            commission: entry.commission,
            client_type: entry.client_type,
            version,
            ip: entry.ip,
            is_superminority: entry.is_superminority,
            rank: entry.rank,
            vote_account_last_update_slot: entry.vote_account_last_update_slot,
            mev_earned: entry.mev_earned,
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct ClientVersionResponse {
    pub major: u8,
    pub minor: u8,
    pub patch: u16,
}

impl ClientVersionResponse {
    pub fn from_client_version(version: ClientVersion) -> Self {
        Self {
            major: version.major,
            minor: version.minor,
            patch: version.patch,
        }
    }
}
