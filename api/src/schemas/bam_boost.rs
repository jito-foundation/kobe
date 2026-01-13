use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;

pub(crate) fn merkle_distributor_address(
    bam_boost_program_id: Pubkey,
    jitosol_mint: Pubkey,
    epoch: u64,
) -> Pubkey {
    let program_id = bam_boost_program_id;
    Pubkey::find_program_address(
        &[
            b"merkle_distributor",
            jitosol_mint.to_bytes().as_slice(),
            epoch.to_le_bytes().as_slice(),
        ],
        &program_id,
    )
    .0
}

/// Response containing the claim proof for a validator
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BamBoostClaimResponse {
    /// The amount to claim
    pub amount: u64,

    /// The claimant's pubkey
    pub claimant: String,

    /// The merkle proof for claiming
    pub proof: Vec<[u8; 32]>,

    /// The merkle root
    pub merkle_root: [u8; 32],

    /// Distributor pubkey
    pub distributor_address: String,
}
