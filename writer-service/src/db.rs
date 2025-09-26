use std::time::{Duration as CoreDuration, Instant};

use backoff::{future::retry, ExponentialBackoff};
use kobe_core::{
    constants::{
        STAKER_REWARDS_COLLECTION_NAME, STAKE_POOL_STATS_COLLECTION_NAME, VALIDATOR_COLLECTION_NAME,
    },
    db_models::{
        mev_rewards::StakerRewards, stake_pool_stats::StakePoolStats, validators::Validator,
    },
};
use log::{error, info, warn};
use mongodb::{
    bson::doc,
    options::{ClientOptions, ReplaceOptions},
    Client as MongodbClient, Collection, Database,
};
use reqwest::Client as ReqwestClient;
use serde::Serialize;
use solana_program::pubkey::Pubkey;

use crate::{
    google_storage, merkle_tree_parser,
    result::Result,
    stake_pool_manager::StakePoolManager,
    tip_distributor_sdk::{GeneratedMerkleTreeCollection, StakeMetaCollection},
};

pub async fn write_to_db<T>(collection: &Collection<T>, items: &Vec<T>) -> Result<()>
where
    T: Serialize + Send + Sync,
{
    let start = Instant::now();
    collection.insert_many(items).await?;

    info!(
        "done writing {:#?} items to db, took {}ms",
        items.len(),
        start.elapsed().as_millis()
    );
    Ok(())
}

pub async fn upsert_to_db(
    collection: &Collection<Validator>,
    items: &[Validator],
    epoch: u64,
) -> Result<()> {
    let start = Instant::now();
    let batch_size = 100;

    let mut replace_options = ReplaceOptions::default();
    replace_options.upsert = Some(true);

    for (i, chunk) in items.chunks(batch_size).enumerate() {
        info!(
            "Processing batch {} of {}",
            i + 1,
            items.len().div_ceil(batch_size)
        );

        for item in chunk {
            collection
                .replace_one(
                    doc! {
                        "epoch": epoch as u32,
                        "vote_account": &item.vote_account
                    },
                    item,
                )
                .with_options(replace_options.clone())
                .await?;
        }

        // Small delay between batches to avoid overwhelming the server
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    info!(
        "done upserting {} items to db, took {}ms",
        items.len(),
        start.elapsed().as_millis()
    );

    Ok(())
}

pub async fn write_mev_claims_info(
    db: &Database,
    target_epoch: u64,
    tip_distribution_program_id: &str,
    priority_fee_distribution_program_id: &str,
    mainnet_gcp_server_names: &[String],
) -> Result<()> {
    // Check if it's time to run else exit early
    let collection = db.collection::<StakerRewards>(STAKER_REWARDS_COLLECTION_NAME);
    let result = collection
        .find_one(doc! {"epoch": (target_epoch) as u32})
        .await?;
    if result.is_some() {
        warn!("MEV claims for epoch {target_epoch} already exist in DB");
        return Ok(());
    }

    let (merkle_tree_uri, stake_meta_uri) =
        google_storage::get_file_uris(target_epoch, mainnet_gcp_server_names).await?;

    let client = ReqwestClient::builder()
        .timeout(CoreDuration::from_secs(600))
        .build()?;

    let backoff = ExponentialBackoff::default();
    let stake_meta_collection_res: std::result::Result<StakeMetaCollection, reqwest::Error> =
        retry(backoff.clone(), || async {
            let res = client
                .get(stake_meta_uri.clone())
                .send()
                .await?
                .json()
                .await;
            Ok(res)
        })
        .await?;
    let merkle_tree_collection_res: std::result::Result<
        GeneratedMerkleTreeCollection,
        reqwest::Error,
    > = retry(backoff, || async {
        let res = client
            .get(merkle_tree_uri.clone())
            .send()
            .await?
            .json()
            .await;
        Ok(res)
    })
    .await?;
    info!("Successfully fetched merkle tree collection");

    info!("Starting merkle tree parsing for epoch {target_epoch}");
    merkle_tree_parser::parse_merkle_tree(
        db,
        target_epoch,
        &merkle_tree_collection_res?,
        &stake_meta_collection_res?,
        tip_distribution_program_id,
        priority_fee_distribution_program_id,
    )
    .await
}

pub async fn write_stake_pool_info(
    db: &Database,
    stake_pool_manager: &StakePoolManager,
    stake_pool_address: &Pubkey,
) -> Result<()> {
    let collection = db.collection::<StakePoolStats>(STAKE_POOL_STATS_COLLECTION_NAME);
    let stake_pool_stats = stake_pool_manager
        .fetch_stake_pool_stats(stake_pool_address)
        .await?;
    info!("{stake_pool_stats:#?}");
    write_to_db(&collection, &vec![stake_pool_stats]).await
}

pub async fn write_validator_info(
    db: &Database,
    stake_pool_manager: &StakePoolManager,
    epoch: u64,
    validator_list_address: &Pubkey,
) -> Result<()> {
    let collection = db.collection::<Validator>(VALIDATOR_COLLECTION_NAME);
    let validators = stake_pool_manager
        .fetch_all_validators(epoch, validator_list_address)
        .await
        .map_err(|e| {
            error!("Cannot write validators to DB: {e:?}");
            e
        })?;
    upsert_to_db(&collection, &validators, epoch).await
}

pub async fn setup_mongo_client(uri: &str) -> Result<MongodbClient> {
    let client_options = ClientOptions::parse(uri).await?;
    Ok(MongodbClient::with_options(client_options)?)
}
