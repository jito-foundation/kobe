use std::{fs::File, path::PathBuf, result};

use serde::{Deserialize, Serialize};

use crate::error::MerkleTreeError;

pub type Result<T> = result::Result<T, MerkleTreeError>;

/// Represents a single entry in a CSV
#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct AirdropEntry {
    /// Pubkey of the claimant;
    ///
    /// - Will be responsible for signing the claim
    /// - Should be node identity key
    pub pubkey: String,

    /// Amount unlocked, (ui amount)
    pub amount: u64,
}

impl AirdropEntry {
    pub fn new(pubkey: String, amount: u64) -> Self {
        Self { pubkey, amount }
    }

    pub fn from_csv_file(path: &PathBuf) -> Result<Vec<Self>> {
        let file = File::open(path)?;
        let mut rdr = csv::Reader::from_reader(file);

        let mut entries = Vec::new();
        for result in rdr.deserialize() {
            let record: AirdropEntry = result.unwrap();
            entries.push(record);
        }

        Ok(entries)
    }

    pub fn from_json_file(path: &PathBuf) -> Result<Vec<Self>> {
        let file = File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let entries: Vec<AirdropEntry> = serde_json::from_reader(reader)?;
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_parsing() {
        let path = PathBuf::from("./test_fixtures/test_csv.csv");
        let entries = AirdropEntry::from_csv_file(&path).expect("Failed to parse CSV");

        assert_eq!(entries.len(), 3);

        assert_eq!(
            entries[0].pubkey,
            "4SX6nqv5VRLMoNfYM5phvHgcBNcBEwUEES4qPPjf1EqS"
        );
        assert_eq!(entries[0].amount, 1000);
    }
}
