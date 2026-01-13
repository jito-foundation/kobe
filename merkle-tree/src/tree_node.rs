use std::str::FromStr;

use serde::{Deserialize, Serialize};
use solana_hash::Hash;
use solana_program::{hash::hashv, pubkey::Pubkey};

use crate::airdrop_entry::AirdropEntry;

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

    // Get total amount of unlocked tokens for this claimant
    // pub fn amount_unlocked(&self) -> u64 {
    //     self.total_unlocked_searcher
    //         .checked_add(self.total_unlocked_validator)
    //         .unwrap()
    //         .checked_add(self.total_unlocked_staker)
    //         .unwrap()
    // }

    // Get total amount of locked tokens for this claimant
    // pub fn amount_locked(&self) -> u64 {
    //     self.total_locked_searcher
    //         .checked_add(self.total_locked_validator)
    //         .unwrap()
    //         .checked_add(self.total_locked_staker)
    //         .unwrap()
    // }
}

/// Converts a ui amount to a token amount (with decimals)
fn ui_amount_to_token_amount(amount: u64) -> u64 {
    amount * 10u64.checked_pow(MINT_DECIMALS).unwrap()
}

impl From<AirdropEntry> for TreeNode {
    fn from(entry: AirdropEntry) -> Self {
        let node = Self {
            claimant: Pubkey::from_str(entry.pubkey.as_str()).unwrap(),
            proof: None,
            amount: ui_amount_to_token_amount(entry.amount),
            // total_locked_staker: 0,
            // total_unlocked_searcher: 0,
            // total_locked_searcher: 0,
            // total_unlocked_validator: 0,
            // total_locked_validator: 0,
        };

        // CSV entry uses UI amounts; we convert to native amounts here
        // let amount =
        // let amount_locked = ui_amount_to_token_amount(entry.amount_locked);
        // match entry.category {
        //     AirdropCategory::Staker => {
        //         node.total_unlocked_staker = amount_unlocked;
        //         node.total_locked_staker = amount_locked;
        //     }
        //     AirdropCategory::Validator => {
        //         node.total_unlocked_validator = amount_unlocked;
        //         node.total_locked_validator = amount_locked;
        //     }
        //     AirdropCategory::Searcher => {
        //         node.total_unlocked_searcher = amount_unlocked;
        //         node.total_locked_searcher = amount_locked;
        //     }
        // }

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
//
//     #[test]
//     fn test_ui_amount_to_token_amount() {
//         let ui_amount = 5;
//         let token_amount = ui_amount_to_token_amount(ui_amount);
//         assert_eq!(token_amount, 5_000_000_000);
//     }
// }
//
