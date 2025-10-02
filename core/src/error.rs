use thiserror::Error;

#[derive(Debug, Error)]
pub enum KobeCoreError {
    #[error(
        "Invalid cluster value: '{0}'. Expected 'testnet', 'mainnet', 'mainnet-beta', or 'devnet'"
    )]
    InvalidCluster(String),
}
