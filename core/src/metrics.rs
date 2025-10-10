use solana_client::{client_error::ClientError, rpc_client::RpcClient};
use solana_metrics::datapoint_info;
use solana_program::pubkey::Pubkey;
use solana_sdk::commitment_config::CommitmentConfig;
use spl_stake_pool_cli::client::get_stake_pool;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetricsError {
    #[error("Client error: {0:?}")]
    RpcError(#[from] ClientError),

    #[error("Stake Pool Error")]
    StakePoolError,
}

pub struct StakePoolMetrics {
    client: RpcClient,
    stake_pool_pubkey: Pubkey,
    cluster_name: String,
}

impl StakePoolMetrics {
    pub async fn new(rpc_url: &String, stake_pool_pubkey: Pubkey, cluster_name: String) -> Self {
        let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());

        Self {
            client,
            stake_pool_pubkey,
            cluster_name,
        }
    }

    pub async fn report_updated_metric(&mut self) -> Result<(), MetricsError> {
        let current_epoch = self.client.get_epoch_info()?.epoch;
        let stake_pool = get_stake_pool(&self.client, &self.stake_pool_pubkey)
            .map_err(|_| MetricsError::StakePoolError)?;

        let is_updated = (stake_pool.last_update_epoch == current_epoch) as i64;
        datapoint_info!("kobe_cranker", ("updated_i", is_updated, i64), "cluster" => &self.cluster_name);
        Ok(())
    }
}
