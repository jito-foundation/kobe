//! DB model for a BAM Delegation Blacklist.

use futures::TryStreamExt;
use mongodb::Collection;
use serde::{Deserialize, Serialize};

use crate::db_models::error::DataStoreError;

#[derive(Clone, Default, Debug, PartialOrd, PartialEq, Deserialize, Serialize)]
pub struct BamDelegationBlacklistEntry {
    /// Vote account address
    vote_account: String,

    /// The epoch number added to blacklist
    added_epoch: u64,
}

#[derive(Clone)]
pub struct BamDelegationBlacklistStore {
    collection: Collection<BamDelegationBlacklistEntry>,
}

impl BamDelegationBlacklistStore {
    pub const COLLECTION: &'static str = "bam_delegation_blacklist";

    /// Initialize a [`BamDelegationBlacklistStore`]
    pub fn new(collection: Collection<BamDelegationBlacklistEntry>) -> Self {
        Self { collection }
    }

    /// Insert a [`BamDelegationBlacklistEntry`]
    pub async fn insert(
        &self,
        entry: BamDelegationBlacklistEntry,
    ) -> Result<(), mongodb::error::Error> {
        self.collection.insert_one(entry, None).await?;
        Ok(())
    }

    /// Find [`BamDelegationBlacklistEntry`] records
    pub async fn find(&self) -> Result<Vec<BamDelegationBlacklistEntry>, DataStoreError> {
        let cursor = self.collection.find(None, None).await?;
        let entries: Vec<BamDelegationBlacklistEntry> = cursor.try_collect().await?;

        Ok(entries)
    }
}
