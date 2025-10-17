use serde::{Deserialize, Serialize};
use validator_history::{
    ClientVersion, MerkleRootUploadAuthority, ValidatorHistory, ValidatorHistoryEntry,
};

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
    pub last_ip_timestamp: u64,

    pub last_version_timestamp: u64,

    /// Total epochs with non-zero vote credits
    pub validator_age: u32,

    /// Last epoch when age was updated
    pub validator_age_last_updated_epoch: u16,

    pub history: Vec<ValidatorHistoryEntryResponse>,
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
            validator_age: acc.validator_age,
            validator_age_last_updated_epoch: acc.validator_age_last_updated_epoch,
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

    /// MEV commission in basis points
    pub mev_commission: u16,

    /// Number of successful votes in current epoch. Not finalized until subsequent epoch
    pub epoch_credits: u32,

    /// Validator commission in points
    pub commission: u8,

    /// 0 if Solana Labs client, 1 if Jito client, >1 if other
    pub client_type: u8,

    /// Client versin
    pub version: ClientVersionResponse,

    /// IP address
    pub ip: [u8; 4],

    /// The enum mapping of the Validator's Tip Distribution Account's merkle root upload authority
    pub merkle_root_upload_authority: MerkleRootUploadAuthorityResponse,

    /// 0 if not a superminority validator, 1 if superminority validator
    pub is_superminority: u8,

    /// rank of validator by stake amount
    pub rank: u32,

    /// Most recent updated slot for epoch credits and commission
    pub vote_account_last_update_slot: u64,

    /// MEV earned, stored as 1/100th SOL. mev_earned = 100 means 1.00 SOL earned
    pub mev_earned: u32,

    /// Priority Fee commission in basis points
    pub priority_fee_commission: u16,

    /// Priority Fee tips that were transferred to the distribution account in lamports
    pub priority_fee_tips: u64,

    /// The total priority fees the validator earned for the epoch.
    pub total_priority_fees: u64,

    /// The number of leader slots the validator had during the epoch
    pub total_leader_slots: u32,

    /// The final number of blocks the validator produced during an epoch
    pub blocks_produced: u32,

    /// The last slot the block data was last updated at
    pub block_data_updated_at_slot: u64,

    /// The enum mapping of the Validator's Tip Distribution Account's merkle root upload authority
    pub priority_fee_merkle_root_upload_authority: MerkleRootUploadAuthorityResponse,
}

#[derive(Default, Clone, Serialize, Deserialize)]
#[repr(u8)]
pub enum MerkleRootUploadAuthorityResponse {
    #[default]
    Unset = u8::MAX,
    Other = 1,
    OldJitoLabs = 2,
    TipRouter = 3,
    DNE = 4,
}

impl From<MerkleRootUploadAuthority> for MerkleRootUploadAuthorityResponse {
    fn from(value: MerkleRootUploadAuthority) -> Self {
        match value {
            MerkleRootUploadAuthority::Unset => MerkleRootUploadAuthorityResponse::Unset,
            MerkleRootUploadAuthority::Other => MerkleRootUploadAuthorityResponse::Other,
            MerkleRootUploadAuthority::OldJitoLabs => {
                MerkleRootUploadAuthorityResponse::OldJitoLabs
            }
            MerkleRootUploadAuthority::TipRouter => MerkleRootUploadAuthorityResponse::TipRouter,
            MerkleRootUploadAuthority::DNE => MerkleRootUploadAuthorityResponse::DNE,
        }
    }
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
            merkle_root_upload_authority: entry.merkle_root_upload_authority.into(),
            is_superminority: entry.is_superminority,
            rank: entry.rank,
            vote_account_last_update_slot: entry.vote_account_last_update_slot,
            mev_earned: entry.mev_earned,
            priority_fee_commission: entry.priority_fee_commission,
            priority_fee_tips: entry.priority_fee_tips,
            total_priority_fees: entry.total_priority_fees,
            total_leader_slots: entry.total_leader_slots,
            blocks_produced: entry.blocks_produced,
            block_data_updated_at_slot: entry.block_data_updated_at_slot,
            priority_fee_merkle_root_upload_authority: entry
                .priority_fee_merkle_root_upload_authority
                .into(),
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
