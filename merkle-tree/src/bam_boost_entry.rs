use std::{fs::File, path::PathBuf, result};

use serde::{Deserialize, Serialize};

use crate::error::MerkleTreeError;

pub type Result<T> = result::Result<T, MerkleTreeError>;

/// Represents a single entry in a CSV
#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct BamBoostEntry {
    /// Pubkey of the claimant; will be responsible for signing the claim
    pub pubkey: String,

    /// Amount
    pub amount: u64,
}

impl BamBoostEntry {
    pub fn new(pubkey: String, amount: u64) -> Self {
        Self { pubkey, amount }
    }

    pub fn new_from_file(path: &PathBuf) -> Result<Vec<Self>> {
        let file = File::open(path)?;
        let mut rdr = csv::Reader::from_reader(file);

        let mut entries = Vec::new();
        for result in rdr.deserialize() {
            let record: BamBoostEntry = result.unwrap();
            entries.push(record);
        }

        Ok(entries)
    }
}
