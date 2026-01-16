use std::time::Instant;

use futures::TryStreamExt;
use mongodb::{bson::doc, options::ReplaceOptions, Collection};
use serde::{Deserialize, Serialize};

use crate::{constants::BAM_BOOST_VALIDATORS_COLLECTION_NAME, db_models::error::DataStoreError};

#[derive(Clone, Serialize, Deserialize, Default, Debug, PartialOrd, PartialEq)]
pub struct BamBoostValidator {
    /// Epoch
    pub epoch: u64,

    /// Identity account
    pub identity_account: String,

    /// Subsidy amount
    pub amount: u64,

    /// Whether claimed or not
    pub claimed: bool,
}

#[derive(Clone)]
pub struct BamBoostValidatorsStore {
    collection: Collection<BamBoostValidator>,
}

impl BamBoostValidatorsStore {
    pub const COLLECTION: &'static str = BAM_BOOST_VALIDATORS_COLLECTION_NAME;

    pub fn new(collection: Collection<BamBoostValidator>) -> Self {
        Self { collection }
    }

    /// Upsert a [`BamBoostValidator`] record
    pub async fn upsert(&self, items: &[BamBoostValidator]) -> Result<(), mongodb::error::Error> {
        let start = Instant::now();
        let batch_size = 100;

        let mut replace_options = ReplaceOptions::default();
        replace_options.upsert = Some(true);

        for (i, chunk) in items.chunks(batch_size).enumerate() {
            log::info!(
                "Processing batch {} of {}",
                i + 1,
                items.len().div_ceil(batch_size)
            );

            for item in chunk {
                self.collection
                    .replace_one(
                        doc! {
                            "epoch": item.epoch as u32,
                            "identity_account": &item.identity_account
                        },
                        item,
                        replace_options.clone(),
                    )
                    .await?;
            }

            // Small delay between batches to avoid overwhelming the server
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        log::info!(
            "done upserting {} items to db, took {}ms",
            items.len(),
            start.elapsed().as_millis()
        );

        Ok(())
    }

    /// Find [`BamBoostValidator`] records
    pub async fn find(&self, epoch: u64) -> Result<Vec<BamBoostValidator>, DataStoreError> {
        let filter = doc! {"epoch": epoch as u32};

        let cursor = self.collection.find(filter, None).await?;
        let validators: Vec<BamBoostValidator> = cursor.try_collect().await?;

        Ok(validators)
    }
}
