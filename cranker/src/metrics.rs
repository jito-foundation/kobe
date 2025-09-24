use solana_client::{client_error::ClientError, nonblocking::rpc_client::RpcClient};
use solana_metrics::datapoint_info;
use solana_program::pubkey::Pubkey;
use solana_sdk::commitment_config::CommitmentConfig;
use spl_stake_pool_cli::client::get_stake_pool;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetricsError {
    #[error("Client error: {0:?}")]
    RpcError(#[from] Box<ClientError>),

    #[error("Stake Pool Error: {0:?}")]
    StakePoolError(String),
}

impl From<ClientError> for MetricsError {
    fn from(value: ClientError) -> Self {
        Self::RpcError(Box::new(value))
    }
}

pub struct StakePoolMetrics {
    client: RpcClient,
    stake_pool_pubkey: Pubkey,
    cluster_name: String,
}

impl StakePoolMetrics {
    pub fn new(rpc_url: String, stake_pool_pubkey: Pubkey, cluster_name: String) -> Self {
        let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());

        Self {
            client,
            stake_pool_pubkey,
            cluster_name,
        }
    }

    pub async fn report_metrics(&mut self) -> Result<(), MetricsError> {
        let current_epoch = self.client.get_epoch_info().await?.epoch;
        let stake_pool = get_stake_pool(&self.client, &self.stake_pool_pubkey)
            .await
            .map_err(|e| MetricsError::StakePoolError(e.to_string()))?;

        let is_updated = (stake_pool.last_update_epoch == current_epoch) as i64;
        datapoint_info!("kobe_cranker", ("updated_i", is_updated, i64), "cluster" => &self.cluster_name);
        Ok(())
    }
}
