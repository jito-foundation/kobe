use kobe_core::db_models::error::DataStoreError;
use thiserror::Error;

pub type Result<T> = core::result::Result<T, QueryResolverError>;

#[derive(Error, Debug)]
pub enum QueryResolverError {
    #[error("querying data store error")]
    DataStoreError(#[from] DataStoreError),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("RPC Error: {0}")]
    RpcError(String),

    #[error("Custom error: {0}")]
    CustomError(String),

    #[error("MongoDB error: {0}")]
    MongoDBError(#[from] mongodb::error::Error),

    #[error("Validator History Error")]
    ValidatorHistoryError(String),
}
