//! DB model for a CB Balance.

use mongodb::{bson::doc, Collection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoinbaseBalance {
    /// Epoch number
    epoch: u64,

    /// Balance in lamports
    cb_balance_lamports: u64,

    /// Total epoch fee from Dune converted to JitoSOL token units
    total_epoch_fee_jitosol: Option<u64>,

    /// CB proportional share of total_epoch_fee in JitoSOL token units
    cb_epoch_fee_jitosol: Option<u64>,
}

impl CoinbaseBalance {
    pub fn new(epoch: u64, cb_balance_lamports: u64) -> Self {
        Self {
            epoch,
            cb_balance_lamports,
            total_epoch_fee_jitosol: None,
            cb_epoch_fee_jitosol: None,
        }
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}

#[derive(Clone)]
pub struct CoinbaseBalanceStore {
    collection: Collection<CoinbaseBalance>,
}

impl CoinbaseBalanceStore {
    pub const COLLECTION: &'static str = "coinbase_balances";

    pub fn new(collection: Collection<CoinbaseBalance>) -> Self {
        Self { collection }
    }

    /// Find [`CoinbaseBalance`] records
    pub async fn find(&self, epoch: u64) -> Result<Option<CoinbaseBalance>, mongodb::error::Error> {
        let filter = doc! {"epoch": epoch as u32};
        self.collection.find_one(filter, None).await
    }
}
