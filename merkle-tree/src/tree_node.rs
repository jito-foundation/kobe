use std::str::FromStr;

use serde::{Deserialize, Serialize};
use solana_hash::Hash;
use solana_program::{hash::hashv, pubkey::Pubkey};

use crate::bam_boost_entry::BamBoostEntry;

pub const MINT_DECIMALS: u32 = 9;

/// Represents the claim information for an account.
#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TreeNode {
    /// Pubkey of the claimant; will be responsible for signing the claim
    pub claimant: Pubkey,

    /// Claimant's proof of inclusion in the Merkle Tree
    pub proof: Option<Vec<[u8; 32]>>,

    /// Total amount allocation
    pub amount: u64,
}

impl TreeNode {
    pub fn hash(&self) -> Hash {
        hashv(&[&self.claimant.to_bytes(), &self.amount().to_le_bytes()])
    }

    /// Return total amount of locked and unlocked amount for this claimant
    pub fn amount(&self) -> u64 {
        self.amount
    }
}

impl From<BamBoostEntry> for TreeNode {
    fn from(entry: BamBoostEntry) -> Self {
        let node = Self {
            claimant: Pubkey::from_str(entry.pubkey.as_str()).unwrap(),
            proof: None,
            amount: entry.amount,
        };

        node
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn test_serialize_tree_node() {
//         let tree_node = TreeNode {
//             claimant: Pubkey::default(),
//             proof: None,
//             amount: 0,
//         };
//         let serialized = serde_json::to_string(&tree_node).unwrap();
//         let deserialized: TreeNode = serde_json::from_str(&serialized).unwrap();
//         assert_eq!(tree_node, deserialized);
//     }
// }
//
