use std::str::FromStr;

use futures::TryStreamExt;
use jito_steward::{
    events::{
        AutoAddValidatorEvent, AutoRemoveValidatorEvent, DecreaseComponents, EpochMaintenanceEvent,
        InstantUnstakeComponents, RebalanceEvent, RebalanceTypeTag, ScoreComponents,
        StateTransition,
    },
    score::{InstantUnstakeComponentsV3, ScoreComponentsV3},
};
use log::info;
use mongodb::{
    bson::{self, doc, DateTime, Document},
    options::FindOneOptions,
    Collection,
};
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;
use solana_signature::Signature;

use crate::constants::STEWARD_EVENTS_COLLECTION_NAME;

#[derive(Debug, Serialize, Deserialize)]
pub struct StewardEvent {
    pub signature: String,
    pub instruction_idx: u32,
    pub event_type: String,
    pub vote_account: Option<String>,
    pub metadata: Option<bson::Document>,
    pub tx_error: Option<String>,
    pub signer: String,
    pub stake_pool: String,
    pub epoch: u64,
    pub slot: u64,
    pub timestamp: Option<DateTime>,
}

impl StewardEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        signature: &Signature,
        instruction_idx: u32,
        event_type: &String,
        vote_account: Option<Pubkey>,
        metadata: Option<Document>,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        epoch: u64,
        timestamp: Option<i64>,
        slot: u64,
    ) -> Self {
        Self {
            signature: signature.to_string(),
            instruction_idx,
            event_type: event_type.to_string(),
            vote_account: vote_account.map(|pk| pk.to_string()),
            metadata,
            tx_error,
            signer: signer.to_string(),
            stake_pool: stake_pool.to_string(),
            epoch,
            timestamp: timestamp.map(|t| DateTime::from_millis(t * 1000)),
            slot,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_rebalance_event(
        event: RebalanceEvent,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        slot: u64,
    ) -> Self {
        let rebalance_string = match event.rebalance_type_tag {
            RebalanceTypeTag::None => "None".to_string(),
            RebalanceTypeTag::Increase => "Increase".to_string(),
            RebalanceTypeTag::Decrease => "Decrease".to_string(),
        };
        let metadata = doc! {
            "rebalance_type_tag": rebalance_string,
            "increase_lamports": event.increase_lamports as i64,
            "decrease_components": {
                "scoring_unstake_lamports": event.decrease_components.scoring_unstake_lamports as i64,
                "instant_unstake_lamports": event.decrease_components.instant_unstake_lamports as i64,
                "stake_deposit_unstake_lamports": event.decrease_components.stake_deposit_unstake_lamports as i64,
                "total_unstake_lamports": event.decrease_components.total_unstake_lamports as i64,
            }
        };

        Self::new(
            signature,
            instruction_idx,
            &"RebalanceEvent".to_string(),
            Some(event.vote_account),
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            event.epoch as u64,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_decrease_components(
        event: DecreaseComponents,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        epoch: u64,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "scoring_unstake_lamports": event.scoring_unstake_lamports as i64,
            "instant_unstake_lamports": event.instant_unstake_lamports as i64,
            "stake_deposit_unstake_lamports": event.stake_deposit_unstake_lamports as i64,
            "total_unstake_lamports": event.total_unstake_lamports as i64,
        };

        Self::new(
            signature,
            instruction_idx,
            &"DecreaseComponents".to_string(),
            None,
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            epoch,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_score_components(
        event: ScoreComponents,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "score": event.score,
            "yield_score": event.yield_score,
            "mev_commission_score": event.mev_commission_score,
            "blacklisted_score": event.blacklisted_score,
            "superminority_score": event.superminority_score,
            "delinquency_score": event.delinquency_score,
            "running_jito_score": event.running_jito_score,
            "commission_score": event.commission_score,
            "historical_commission_score": event.historical_commission_score,
            "vote_credits_ratio": event.vote_credits_ratio,
        };

        Self::new(
            signature,
            instruction_idx,
            &"ScoreComponents".to_string(),
            Some(event.vote_account),
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            event.epoch as u64,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_score_components_v3(
        event: ScoreComponentsV3,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "score": event.score,
            "yield_score": event.yield_score,
            "mev_commission_score": event.mev_commission_score,
            "blacklisted_score": event.blacklisted_score,
            "superminority_score": event.superminority_score,
            "delinquency_score": event.delinquency_score,
            "running_jito_score": event.running_jito_score,
            "commission_score": event.commission_score,
            "historical_commission_score": event.historical_commission_score,
            "merkle_root_upload_authority_score": event.merkle_root_upload_authority_score,
            "vote_credits_ratio": event.vote_credits_ratio,
            "priority_fee_commission_score": event.priority_fee_commission_score,
            "priority_fee_merkle_root_upload_authority_score": event.priority_fee_merkle_root_upload_authority_score,
            "details": doc! {
                "max_mev_commission": event.details.max_mev_commission as i32,
                "max_mev_commission_epoch": event.details.max_mev_commission_epoch as i32,
                "superminority_epoch": event.details.superminority_epoch as i32,
                "delinquency_ratio": event.details.delinquency_ratio,
                "delinquency_epoch": event.details.delinquency_epoch as i32,
                "max_commission": event.details.max_commission as i32,
                "max_commission_epoch": event.details.max_commission_epoch as i32,
                "max_historical_commission": event.details.max_historical_commission as i32,
                "max_historical_commission_epoch": event.details.max_historical_commission_epoch as i32,
                "avg_priority_fee_commission": event.details.avg_priority_fee_commission as i32,
                "max_priority_fee_commission_epoch": event.details.max_priority_fee_commission_epoch as i32
            }
        };

        Self::new(
            signature,
            instruction_idx,
            &"ScoreComponentsV3".to_string(),
            Some(event.vote_account),
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            event.epoch as u64,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_instant_unstake_components(
        event: InstantUnstakeComponents,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "instant_unstake": event.instant_unstake,
            "delinquency_check": event.delinquency_check,
            "commission_check": event.commission_check,
            "mev_commission_check": event.mev_commission_check,
            "is_blacklisted": event.is_blacklisted,
        };

        Self::new(
            signature,
            instruction_idx,
            &"InstantUnstakeComponents".to_string(),
            Some(event.vote_account),
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            event.epoch as u64,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_instant_unstake_components_v3(
        event: InstantUnstakeComponentsV3,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "instant_unstake": event.instant_unstake,
            "delinquency_check": event.delinquency_check,
            "commission_check": event.commission_check,
            "mev_commission_check": event.mev_commission_check,
            "is_blacklisted": event.is_blacklisted,
            "is_bad_merkle_root_upload_authority": event.is_bad_merkle_root_upload_authority,
            "is_bad_priority_fee_merkle_root_upload_authority": event.is_bad_priority_fee_merkle_root_upload_authority,
            "details": doc! {
                "epoch_credits_latest": event.details.epoch_credits_latest as i64,
                "vote_account_last_update_slot": event.details.vote_account_last_update_slot as i64,
                "total_blocks_latest": event.details.total_blocks_latest as i32,
                "cluster_history_slot_index": event.details.cluster_history_slot_index as i64,
                "commission": event.details.commission as i32,
                "mev_commission": event.details.mev_commission as i32
            }
        };

        Self::new(
            signature,
            instruction_idx,
            &"InstantUnstakeComponentsV3".to_string(),
            Some(event.vote_account),
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            event.epoch as u64,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_state_transition(
        event: StateTransition,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "slot": event.slot as i64,
            "previous_state": event.previous_state,
            "new_state": event.new_state,
        };

        Self::new(
            signature,
            instruction_idx,
            &"StateTransition".to_string(),
            None,
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            event.epoch,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_auto_add_validator_event(
        event: AutoAddValidatorEvent,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        epoch: u64,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "validator_list_index": event.validator_list_index as i64,
        };

        Self::new(
            signature,
            instruction_idx,
            &"AutoAddValidatorEvent".to_string(),
            Some(event.vote_account),
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            epoch,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_epoch_maintenance_event(
        event: EpochMaintenanceEvent,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        epoch: u64,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "validator_index_to_remove": event.validator_index_to_remove.map(|idx| idx as i64),
            "validator_list_length": event.validator_list_length as i64,
            "num_pool_validators": event.num_pool_validators as i64,
            "validators_to_remove": event.validators_to_remove as i64,
            "validators_to_add": event.validators_to_add as i64,
            "maintenance_complete": event.maintenance_complete,
        };

        Self::new(
            signature,
            instruction_idx,
            &"EpochMaintenanceEvent".to_string(),
            None,
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            epoch,
            timestamp,
            slot,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_auto_remove_validator_event(
        event: AutoRemoveValidatorEvent,
        signature: &Signature,
        instruction_idx: u32,
        tx_error: Option<String>,
        signer: &Pubkey,
        stake_pool: &Pubkey,
        timestamp: Option<i64>,
        epoch: u64,
        slot: u64,
    ) -> Self {
        let metadata = doc! {
            "validator_list_index": event.validator_list_index as i64,
            "vote_account_closed": event.vote_account_closed,
            "stake_account_deactivated": event.stake_account_deactivated,
        };

        Self::new(
            signature,
            instruction_idx,
            &"AutoRemoveValidatorEvent".to_string(),
            Some(event.vote_account),
            Some(metadata),
            tx_error,
            signer,
            stake_pool,
            epoch,
            timestamp,
            slot,
        )
    }
}

#[derive(Clone)]
pub struct StewardEventsStore {
    collection: Collection<StewardEvent>,
}

impl StewardEventsStore {
    pub const COLLECTION: &'static str = STEWARD_EVENTS_COLLECTION_NAME;

    pub fn new(collection: Collection<StewardEvent>) -> Self {
        Self { collection }
    }

    pub async fn insert(&self, event: StewardEvent) -> Result<(), mongodb::error::Error> {
        self.collection.insert_one(event, None).await?;
        Ok(())
    }

    pub async fn upsert(&self, event: StewardEvent) -> Result<(), mongodb::error::Error> {
        let update = doc! { "$set": bson::to_document(&event)? };
        let filter = doc! { "signature": &event.signature, "event_type": &event.event_type, "vote_account": &event.vote_account };
        let options = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();
        self.collection.update_one(filter, update, options).await?;
        Ok(())
    }

    pub async fn bulk_upsert(
        &self,
        events: Vec<StewardEvent>,
    ) -> Result<(), mongodb::error::Error> {
        info!("upserting {} steward events", events.len());
        for event in events {
            self.upsert(event).await?;
        }
        Ok(())
    }

    pub async fn get_latest_signature_and_slot(
        &self,
    ) -> Result<Option<(Signature, u64)>, mongodb::error::Error> {
        let options = FindOneOptions::builder().sort(doc! { "slot": -1 }).build();

        let result = self.collection.find_one(None, options).await?;

        match result {
            Some(event) => {
                let signature = Signature::from_str(&event.signature).map_err(|e| {
                    mongodb::error::Error::from(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to parse signature: {e}"),
                    ))
                })?;
                Ok(Some((signature, event.slot)))
            }
            None => Ok(None),
        }
    }

    pub async fn find_by_vote_account(
        &self,
        vote_account: &String,
        limit: Option<i64>,
        skip: Option<u64>,
    ) -> Result<Vec<StewardEvent>, mongodb::error::Error> {
        let filter = doc! { "vote_account": vote_account.to_string() };
        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "slot": -1 })
            .limit(limit)
            .skip(skip)
            .build();
        let mut cursor = self.collection.find(filter, options).await?;

        let mut events = Vec::new();
        while let Some(event) = cursor.try_next().await? {
            events.push(event);
        }

        Ok(events)
    }

    pub async fn find_by_event_type(
        &self,
        event_type: &str,
        limit: Option<i64>,
        skip: Option<u64>,
    ) -> Result<Vec<StewardEvent>, mongodb::error::Error> {
        let filter = doc! { "event_type": event_type };
        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "slot": -1 })
            .limit(limit)
            .skip(skip)
            .build();
        let mut cursor = self.collection.find(filter, options).await?;

        let mut events = Vec::new();
        while let Some(event) = cursor.try_next().await? {
            events.push(event);
        }

        Ok(events)
    }

    pub async fn find_by_epoch(
        &self,
        epoch: u64,
    ) -> Result<Vec<StewardEvent>, mongodb::error::Error> {
        let filter = doc! { "epoch": epoch as i64 };
        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "slot": -1 })
            .build();
        let mut cursor = self.collection.find(filter, options).await?;

        let mut events = Vec::new();
        while let Some(event) = cursor.try_next().await? {
            events.push(event);
        }

        Ok(events)
    }

    pub async fn find_steward_events(
        &self,
        event_type: Option<String>,
        vote_account: Option<String>,
        epoch: Option<u64>,
        limit: i64,
        skip: u64,
    ) -> Result<Vec<StewardEvent>, mongodb::error::Error> {
        let mut filter = Document::new();

        if let Some(event_type) = event_type {
            match event_type.as_str() {
                "ScoreComponents" => {
                    filter.insert(
                        "event_type",
                        doc! { "$in": ["ScoreComponents", "ScoreComponentsV2", "ScoreComponentsV3"] },
                    );
                }
                "InstantUnstakeComponents" => {
                    filter.insert(
                        "event_type",
                        doc! { "$in": ["InstantUnstakeComponents", "InstantUnstakeComponentsV2", "InstantUnstakeComponentsV3"] },
                    );
                }
                _ => {
                    filter.insert("event_type", event_type);
                }
            }
        }
        if let Some(vote_account) = vote_account {
            filter.insert("vote_account", vote_account);
        }
        if let Some(epoch) = epoch {
            filter.insert("epoch", epoch as i64);
        }

        let options = mongodb::options::FindOptions::builder()
            .sort(doc! { "slot": -1 })
            .limit(limit)
            .skip(skip)
            .build();

        let mut cursor = self.collection.find(filter, options).await?;

        let mut events = Vec::new();
        while let Some(event) = cursor.try_next().await? {
            events.push(event);
        }

        Ok(events)
    }
}
