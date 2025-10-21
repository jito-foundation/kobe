use std::collections::HashMap;

use chrono::{serde::ts_seconds, DateTime, Utc};
use constants::STAKE_POOL_STATS_COLLECTION_NAME;
use futures_util::StreamExt;
use mongodb::{bson, bson::doc, Collection};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::constants;
use crate::db_models::error::DataStoreError;

#[derive(Error, Debug, Clone)]
pub enum StakePoolStatsError {
    #[error("num_epochs is zero")]
    NumEpochsZero,
    #[error("docs vector is empty")]
    EmptyDocs,
}

#[derive(Clone, Deserialize, Serialize, Default, Debug, PartialOrd, PartialEq)]
pub struct StakePoolStats {
    pub epoch: u64,
    pub num_deposits: u64, // number of users who have deposited into the pool
    pub reserve_balance: u64,
    #[serde(with = "ts_seconds")]
    pub timestamp: DateTime<Utc>,
    pub total_solana_lamports: u64, // Pool Sol
    pub total_pool_lamports: u64,   // Pool Jitosol
    pub mev_rewards: u64,
    pub apy: f64,
    pub num_validators: u32,
    // fees collected in jitoSOL
    pub fees_collected: Option<f64>, // Optional because field added in mid November '22
    pub total_network_staked_lamports: Option<u64>,
}

#[derive(Clone)]
pub struct StakePoolStatsStore {
    collection: Collection<StakePoolStats>,
}

impl StakePoolStatsStore {
    pub const COLLECTION: &'static str = STAKE_POOL_STATS_COLLECTION_NAME;

    pub fn new(collection: Collection<StakePoolStats>) -> Self {
        Self { collection }
    }

    /// Groups documents into daily buckets between the provided time range.
    /// Returns the last document in each group.
    pub async fn aggregate(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<StakePoolStats>, DataStoreError> {
        let pipeline = vec![
            // 1. Fetch all documents b/w the provided date range inclusively.
            doc! {
               "$match": {
                  "timestamp": { "$gte": start.timestamp(), "$lte": end.timestamp() }
               }
            },
            doc! { "$sort": { "timestamp": 1 } },
            // 3. Group documents by date into daily buckets and return the last document in each group.
            doc! {
                "$group":
                {
                    "_id": { "$dateToString": { "format": "%Y-%m-%d", "date": {"$toDate": {"$multiply": [1000, "$timestamp"]}}}}, // Multiply by 1000 to convert to millis.
                    "num_deposits": { "$last": "$num_deposits" },
                    "reserve_balance": { "$last": "$reserve_balance" },
                    "total_solana_lamports": { "$last": "$total_solana_lamports" },
                    "total_pool_lamports": { "$last": "$total_pool_lamports" },
                    "mev_rewards": { "$last": "$mev_rewards" },
                    "apy": { "$last": "$apy" },
                    "num_validators": { "$last": "$num_validators" },
                    "epoch": { "$last": "$epoch" },
                    "timestamp": { "$last": "$timestamp" },
                }
            },
            doc! { "$sort": { "_id": 1 }},
        ];

        let mut cursor = self.collection.aggregate(pipeline, None).await?;
        let mut docs = vec![];

        while let Some(maybe_doc) = cursor.next().await {
            let doc: StakePoolStats = bson::from_document(maybe_doc?)?;
            docs.push(doc);
        }

        Self::calculate_moving_avg_apy(&docs, 10).map_err(DataStoreError::StakePoolStatsError)
    }

    /// Calculates moving average apy based on past num_epochs epochs
    /// Assumes epochs in `docs` are sorted in ascending order.
    pub fn calculate_moving_avg_apy(
        docs: &Vec<StakePoolStats>,
        num_epochs: usize,
    ) -> Result<Vec<StakePoolStats>, StakePoolStatsError> {
        // Check for error conditions
        if num_epochs == 0 {
            return Err(StakePoolStatsError::NumEpochsZero);
        }
        if docs.is_empty() {
            return Err(StakePoolStatsError::EmptyDocs);
        }

        let mut epoch_apy_avgs: HashMap<u64, f64> = HashMap::new();
        let mut epoch_counts: HashMap<u64, u64> = HashMap::new();

        for stats in docs {
            let count = epoch_counts.entry(stats.epoch).or_insert(0);
            let total_apy = epoch_apy_avgs.entry(stats.epoch).or_insert(0.0);

            *total_apy += stats.apy;
            *count += 1;
        }

        // Calculate the averages
        for (epoch, total_apy) in &mut epoch_apy_avgs {
            *total_apy /= epoch_counts[epoch] as f64;
        }

        // Calculate the moving averages of the previous num_epochs-1 epochs.
        let mut moving_average_apy: HashMap<u64, f64> = HashMap::new();
        let mut sorted_epochs: Vec<_> = docs.clone().iter().map(|s| s.epoch).collect();

        sorted_epochs.sort();
        sorted_epochs.dedup(); // We only need each epoch once

        for epoch in num_epochs..sorted_epochs.len() {
            let sum: f64 = sorted_epochs[epoch - num_epochs + 1..=epoch]
                .iter()
                .map(|&e| epoch_apy_avgs[&e])
                .sum();
            let average = sum / (num_epochs as f64);
            moving_average_apy.insert(sorted_epochs[epoch], average);
        }

        // Create a new list of StakePoolStats with the `apy` replaced by the moving average.
        let mut new_stake_pool_stats: Vec<StakePoolStats> = Vec::new();

        for stats in docs {
            let mut new_stats = stats.clone(); // Clone the current stats
            if let Some(average_apy) = moving_average_apy.get(&stats.epoch) {
                new_stats.apy = *average_apy; // Replace the apy with the moving average
            }
            new_stake_pool_stats.push(new_stats);
        }
        Ok(new_stake_pool_stats)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    #[test]
    fn test_calculate_moving_avg_apy_simple() {
        let now = Utc::now();
        let mut docs = Vec::new();
        for i in 0..20 {
            docs.push(StakePoolStats {
                epoch: i,
                num_deposits: 1,
                reserve_balance: 1,
                timestamp: now + Duration::days(i as i64),
                total_solana_lamports: 1,
                total_pool_lamports: 1,
                mev_rewards: 1,
                apy: 0.5,
                num_validators: 1,
                fees_collected: None,
                total_network_staked_lamports: None,
            });
        }

        let new_docs = StakePoolStatsStore::calculate_moving_avg_apy(&docs, 10).unwrap();
        for i in 10..20 {
            let expected_apy = 0.5;
            assert_eq!(new_docs[i as usize].apy, expected_apy);
        }
    }

    #[test]
    fn test_calculate_moving_avg_apy() {
        let now = Utc::now();
        let mut docs = Vec::new();
        for i in 0..20 {
            docs.push(StakePoolStats {
                epoch: i,
                num_deposits: 1,
                reserve_balance: 1,
                timestamp: now + Duration::days(i as i64),
                total_solana_lamports: 1,
                total_pool_lamports: 1,
                mev_rewards: 1,
                apy: i as f64,
                num_validators: 1,
                fees_collected: None,
                total_network_staked_lamports: None,
            });
        }

        let new_docs = StakePoolStatsStore::calculate_moving_avg_apy(&docs, 10).unwrap();
        for i in 10..20 {
            let expected_apy = (((i - 9)..=i).map(|x| x as f64).sum::<f64>()) / 10.0;
            assert_eq!(new_docs[i as usize].apy, expected_apy);
        }
    }

    #[test]
    fn test_calculate_moving_avg_apy_multi_day_epoch() {
        let now = Utc::now();
        let mut docs = Vec::new();
        for i in 0..20 {
            // Add two documents per epoch with APYs that average out to `i`
            for &apy in &[i as f64 - 0.1, i as f64 + 0.1] {
                docs.push(StakePoolStats {
                    epoch: i as u64,
                    num_deposits: 1,
                    reserve_balance: 1,
                    timestamp: now + Duration::days((2 * i) as i64), // Adjusted timestamp to keep docs in order
                    total_solana_lamports: 1,
                    total_pool_lamports: 1,
                    mev_rewards: 1,
                    apy,
                    num_validators: 1,
                    fees_collected: None,
                    total_network_staked_lamports: None,
                });
            }
        }

        let new_docs = StakePoolStatsStore::calculate_moving_avg_apy(&docs, 10).unwrap();

        for i in 10..20 {
            let expected_apy = (((i - 9)..=i).map(|x| x as f64).sum::<f64>()) / 10.0;
            // Check that the APYs for both documents in each epoch are correctly calculated
            assert_eq!(new_docs[(i * 2) as usize].apy, expected_apy);
            assert_eq!(new_docs[(i * 2 + 1) as usize].apy, expected_apy);
        }
    }

    #[test]
    fn test_calculate_moving_avg_apy_num_epochs_greater_than_records() {
        // When num_epochs is greater than number of records
        let now = Utc::now();
        let mut docs = Vec::new();
        for i in 0..5 {
            docs.push(StakePoolStats {
                epoch: i as u64,
                num_deposits: 1,
                reserve_balance: 1,
                timestamp: now + Duration::days(i as i64),
                total_solana_lamports: 1,
                total_pool_lamports: 1,
                mev_rewards: 1,
                apy: i as f64,
                num_validators: 1,
                fees_collected: None,
                total_network_staked_lamports: None,
            });
        }
        let new_docs = StakePoolStatsStore::calculate_moving_avg_apy(&docs, 10).unwrap();
        assert_eq!(new_docs, docs);
    }

    #[test]
    fn test_calculate_moving_avg_apy_errors() {
        let now = Utc::now();
        let mut docs = Vec::new();
        for i in 0..10 {
            docs.push(StakePoolStats {
                epoch: i,
                num_deposits: 1,
                reserve_balance: 1,
                timestamp: now + Duration::days(i as i64),
                total_solana_lamports: 1,
                total_pool_lamports: 1,
                mev_rewards: 1,
                apy: i as f64,
                num_validators: 1,
                fees_collected: None,
                total_network_staked_lamports: None,
            });
        }

        // Test for num_epochs zero error
        match StakePoolStatsStore::calculate_moving_avg_apy(&docs, 0) {
            Err(StakePoolStatsError::NumEpochsZero) => assert!(!docs.is_empty()),
            _ => unreachable!(),
        }

        // Test for empty docs vector error
        let empty_docs = Vec::new();
        match StakePoolStatsStore::calculate_moving_avg_apy(&empty_docs, 10) {
            Err(StakePoolStatsError::EmptyDocs) => assert!(!docs.is_empty()),
            _ => unreachable!(),
        }
    }
}
