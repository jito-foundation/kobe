use std::{str::FromStr, sync::Arc};

use borsh::BorshDeserialize;
use jito_bam_boost_merkle_tree::bam_boost_entry::BamBoostEntry;
use jito_program_client::bam_boost::config::Config;
use kobe_core::{
    constants::JITOSOL_MINT,
    db_models::bam_boost_validators::{BamBoostValidator, BamBoostValidatorsStore},
    validators_app::Cluster,
};
use mongodb::Database;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

use crate::result::{AppError, Result};

pub struct BamBoostManager {
    /// RPC client
    pub rpc_client: Arc<RpcClient>,

    /// Cluster [Mainnet, Testnet, Devnet]
    pub cluster: Cluster,

    /// Jito BAM Boost Program ID
    pub jito_bam_boost_program_id: Pubkey,
}

impl BamBoostManager {
    pub fn new(rpc_client: Arc<RpcClient>, cluster: Cluster) -> Self {
        Self {
            rpc_client,
            cluster,
            jito_bam_boost_program_id: Pubkey::from_str(
                "BoostxbPp2ENYHGcTLYt1obpcY13HE4NojdqNWdzqSSb",
            )
            .unwrap(),
        }
    }

    fn config_address(&self) -> Pubkey {
        Pubkey::find_program_address(&[b"config"], &self.jito_bam_boost_program_id).0
    }

    fn distributor_address(&self, mint: Pubkey, epoch: u64) -> Pubkey {
        Pubkey::find_program_address(
            &[
                b"merkle_distributor",
                mint.to_bytes().as_ref(),
                epoch.to_le_bytes().as_ref(),
            ],
            &self.jito_bam_boost_program_id,
        )
        .0
    }

    fn claim_status_address(&self, claimant: Pubkey, distributor: Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[
                b"claim_status",
                claimant.to_bytes().as_ref(),
                distributor.to_bytes().as_ref(),
            ],
            &self.jito_bam_boost_program_id,
        )
        .0
    }

    async fn fetch_bam_boost_entries(&self, epoch: u64) -> Result<Vec<BamBoostEntry>> {
        let network = match self.cluster {
            Cluster::MainnetBeta => "mainnet",
            Cluster::Testnet => "testnet",
            Cluster::Devnet => {
                return Err(AppError::InvalidOperation(
                    "Failed to read cluster".to_string(),
                ))
            }
        };

        let url = format!(
            "https://storage.googleapis.com/jito-bam-boost/{network}/{epoch}/merkle_tree.json",
        );

        log::info!("Fetching merkle tree from: {url}");

        // Download the merkle tree JSON from GCS
        let response = match reqwest::get(&url).await {
            Ok(resp) => resp,
            Err(e) => {
                log::error!("Failed to fetch merkle tree: {e}");
                return Err(AppError::FileNotFound(
                    "Failed to fetch merkle tree ({url}: {e}".to_string(),
                ));
            }
        };

        if !response.status().is_success() {
            log::error!("Merkle tree not found: status {}", response.status());
            return Err(AppError::InvalidOperation(format!(
                "Merkle tree not found for network {network} epoch {epoch}",
            )));
        }

        let response_json = response.json().await.unwrap();

        Ok(response_json)
    }

    pub async fn write_bam_boost_info(&self, db: &Database) -> Result<()> {
        let epoch_info = self.rpc_client.get_epoch_info().await?;
        let current_epoch = epoch_info.epoch;
        let bam_boost_collection =
            db.collection::<BamBoostValidator>(BamBoostValidatorsStore::COLLECTION);
        let bam_boost_store = BamBoostValidatorsStore::new(bam_boost_collection);

        let bam_boost_config_address = self.config_address();
        let bam_boost_config_acc = self
            .rpc_client
            .get_account(&bam_boost_config_address)
            .await?;
        let bam_boost_config = Config::deserialize(&mut bam_boost_config_acc.data.as_slice())?;

        let mut bam_boost_validators = Vec::new();

        for epoch in current_epoch - bam_boost_config.clawback_delay_epochs..current_epoch {
            let epoch_bam_boost_entries = self.fetch_bam_boost_entries(epoch).await?;

            for entry in epoch_bam_boost_entries {
                let distributor_pda =
                    self.distributor_address(Pubkey::from_str(JITOSOL_MINT).unwrap(), epoch);

                let claim_status_pda = self.claim_status_address(
                    Pubkey::from_str(&entry.pubkey).unwrap(),
                    distributor_pda,
                );
                let claim_status = self.rpc_client.get_account(&claim_status_pda).await;

                let bam_boost_validator = BamBoostValidator {
                    epoch: current_epoch,
                    identity_account: entry.pubkey.to_string(),
                    amount: entry.amount,
                    claimed: claim_status.is_ok(),
                };
                bam_boost_validators.push(bam_boost_validator);
            }
        }

        bam_boost_store.upsert(&bam_boost_validators).await?;

        Ok(())
    }
}
