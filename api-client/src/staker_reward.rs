use serde::Deserialize;
use solana_pubkey::Pubkey;

#[derive(Debug, Deserialize)]
pub struct StakerReward {
    /// Stake authority pubkey
    ///
    /// Filter by stake authority public key
    pub stake_authority: Pubkey,
}
