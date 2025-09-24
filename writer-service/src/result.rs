use std::io::Error as IoError;

use backoff::Error as BackoffError;
use log::SetLoggerError;
use mongodb::error::Error as MongoError;
use reqwest::Error as ReqwestError;
use serde_json::Error as JsonError;
use solana_client::client_error::ClientError;
use solana_program::pubkey::{ParsePubkeyError, PubkeyError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Network request failed: {0}")]
    Network(#[from] ReqwestError),

    #[error("Database operation failed: {0}")]
    Database(#[from] MongoError),

    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] JsonError),

    #[error("Input/output error: {0}")]
    Io(#[from] IoError),

    #[error("Public key error: {0}")]
    PublicKey(#[from] PubkeyError),

    #[error("Failed to perform operation due to missing data: {0}")]
    NotFound(String),

    #[error("Invalid operation attempted: {0}")]
    InvalidOperation(String),

    #[error("Internal error: {0}")]
    Internal(#[from] InternalError),

    #[error("Parse Pubkey Error: {0}")]
    ParsePubkeyError(#[from] ParsePubkeyError),

    #[error("SetLogger Error: {0}")]
    SetLoggerError(#[from] SetLoggerError),

    #[error("Malformed Merkle Tree")]
    MalformedMerkleTreeError,

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Slot not found")]
    SlotNotFound,

    #[error("ClientError")]
    ClientError(#[from] Box<ClientError>),

    #[error("Backoff Error")]
    BackoffError(String),

    #[error("Empty Fee Account Balance: {0}")]
    EmptyFeeAccountBalance(String),

    #[error("Join errorr")]
    JoinError(#[from] tokio::task::JoinError),
}

impl From<BackoffError<ClientError>> for AppError {
    fn from(error: BackoffError<ClientError>) -> Self {
        match error {
            BackoffError::Permanent(e) => AppError::BackoffError(e.to_string()),
            BackoffError::Transient { err, .. } => AppError::BackoffError(err.to_string()),
        }
    }
}

impl From<Box<dyn std::error::Error>> for AppError {
    fn from(error: Box<dyn std::error::Error>) -> Self {
        AppError::Internal(InternalError::Miscellaneous(error.to_string()))
    }
}

impl From<ClientError> for AppError {
    fn from(value: ClientError) -> Self {
        AppError::ClientError(Box::new(value))
    }
}

impl From<String> for AppError {
    fn from(error: String) -> Self {
        AppError::Internal(InternalError::Miscellaneous(error))
    }
}

#[derive(Error, Debug)]
pub enum InternalError {
    #[error("Timeout reached during operation")]
    Timeout,

    #[error("Configuration issue: {0}")]
    Configuration(String),

    #[error("Miscellaneous error: {0}")]
    Miscellaneous(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
