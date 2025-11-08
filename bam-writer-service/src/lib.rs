use std::{collections::HashSet, sync::Arc};

use bam_api_client::client::BamApiClient;
use kobe_core::db_models::{
    bam_epoch_metric::{BamEpochMetric, BamEpochMetricStore},
    bam_validator::{BamValidator, BamValidatorStore},
};
use solana_client::nonblocking::rpc_client::RpcClient;

pub struct BamWriterService {
    /// RPC Client
    rpc_client: Arc<RpcClient>,

    /// BAM API client
    bam_api_client: BamApiClient,

    /// Bam epoch metric store
    bam_epoch_metric_store: BamEpochMetricStore,

    /// Bam validator store
    bam_validator_store: BamValidatorStore,
}

impl BamWriterService {
    /// Initialize [`BamWriterService`]
    pub fn new(
        rpc_client: RpcClient,
        bam_api_client: BamApiClient,
        bam_epoch_metric_store: BamEpochMetricStore,
        bam_validator_store: BamValidatorStore,
    ) -> Self {
        Self {
            rpc_client: Arc::new(rpc_client),
            bam_api_client,
            bam_epoch_metric_store,
            bam_validator_store,
        }
    }

    /// Run [`BamWriterService`]
    pub async fn run(&self) -> anyhow::Result<()> {
        let epoch_info = self.rpc_client.get_epoch_info().await?;
        let epoch = epoch_info.epoch;

        let bam_node_validators = self.bam_api_client.get_validators().await?;
        let vote_accounts = self.rpc_client.get_vote_accounts().await?;

        let bam_total_network_stake_weight: f64 = bam_node_validators.iter().map(|v| v.stake).sum();
        let eligible_bam_validator_count = bam_node_validators.iter().count();

        let delinquent_vote_accounts: HashSet<String> = HashSet::from_iter(
            vote_accounts
                .delinquent
                .iter()
                .map(|v| v.node_pubkey.clone())
                .collect::<Vec<String>>(),
        );

        let mut bam_validators = Vec::new();
        for bam_validator in bam_node_validators {
            for vote_account in vote_accounts.current.iter() {
                if bam_validator.validator_pubkey.eq(&vote_account.node_pubkey) {
                    let bam_validator = BamValidator::new(
                        epoch,
                        vote_account.vote_pubkey.clone(),
                        bam_validator.stake,
                        delinquent_vote_accounts.contains(&vote_account.node_pubkey),
                        false, // FIXME
                    );
                    bam_validators.push(bam_validator);
                }
            }
        }

        let bam_epoch_metric = BamEpochMetric::new(
            epoch,
            bam_total_network_stake_weight,
            0, // FIXME
            eligible_bam_validator_count as u64,
        );

        self.bam_epoch_metric_store.insert(bam_epoch_metric).await?;
        self.bam_validator_store.bulk_upsert(bam_validators).await?;

        Ok(())
    }
}
