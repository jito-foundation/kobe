use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_clock::Slot;
use solana_commitment_config::CommitmentConfig;
use solana_rpc_client_api::client_error::Error as RpcError;
use solana_signature::Signature;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding};
use std::iter::{Map, Take};
use std::time::Duration;
use tokio_retry::strategy::{jitter, FibonacciBackoff};
use tokio_retry::Retry;

pub const MAX_RPC_RETRIES: usize = 10;
type RetryStrategy = Take<Map<FibonacciBackoff, fn(Duration) -> Duration>>;

pub fn retry() -> RetryStrategy {
    FibonacciBackoff::from_millis(200)
        .factor(1)
        .max_delay(Duration::from_secs(60)) // cap at 5 seconds
        .map(jitter as fn(Duration) -> Duration) // explicitly cast jitter to fn pointer type
        .take(MAX_RPC_RETRIES)
}

pub async fn retry_get_transactions(
    rpc_client: &RpcClient,
    transaction_signatures: &[Signature],
) -> Result<Vec<EncodedConfirmedTransactionWithStatusMeta>, RpcError> {
    let txes = Retry::spawn(retry(), || {
        get_signatures_internal(rpc_client, transaction_signatures)
    })
    .await?;

    Ok(txes)
}

async fn get_signatures_internal(
    rpc_client: &RpcClient,
    transaction_signatures: &[Signature],
) -> Result<Vec<EncodedConfirmedTransactionWithStatusMeta>, RpcError> {
    let config = RpcTransactionConfig {
        commitment: CommitmentConfig::finalized().into(),
        encoding: UiTransactionEncoding::Base64.into(),
        max_supported_transaction_version: Some(0),
    };

    let mut temp_txs = vec![];
    for signature in transaction_signatures.iter() {
        let tx = rpc_client
            .get_transaction_with_config(signature, config)
            .await?;
        temp_txs.push(tx);
    }
    Ok(temp_txs)
}

pub async fn retry_get_slot(rpc_client: &RpcClient) -> Result<Slot, RpcError> {
    Retry::spawn(retry(), || rpc_client.get_slot()).await
}
