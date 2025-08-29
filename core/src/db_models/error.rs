use crate::db_models::stake_pool_stats::StakePoolStatsError;
use mongodb::bson;
use solana_pubkey::ParsePubkeyError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DataStoreError {
    #[error("deserialization error")]
    DeserializationError(#[from] bson::de::Error),

    #[error("mongo client error")]
    MongoClientError(#[from] mongodb::error::Error),

    #[error("stake pool stats error")]
    StakePoolStatsError(#[from] StakePoolStatsError),

    #[error("No results found")]
    NoResultsFound,

    #[error("Invalid Pubkey")]
    InvalidPubkey(#[from] ParsePubkeyError),
}
