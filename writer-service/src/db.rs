use std::{
    collections::HashSet,
    time::{Duration as CoreDuration, Instant},
};

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
    bson::{self, doc, Bson},
    options::{ClientOptions, UpdateOptions},
    Client as MongodbClient, Collection, Database,
};
use reqwest::Client as ReqwestClient;
use serde::Serialize;
use solana_program::pubkey::Pubkey;

use crate::{
    google_storage, merkle_tree_parser,
    result::{AppError, Result},
    stake_pool_manager::StakePoolManager,
    tip_distributor_sdk::{GeneratedMerkleTreeCollection, StakeMetaCollection},
};

pub async fn write_to_db<T>(collection: &Collection<T>, items: &Vec<T>) -> Result<()>
where
    T: Serialize,
{
    let start = Instant::now();
    collection.insert_many(items, None).await?;

    info!(
        "done writing {:#?} items to db, took {}ms",
        items.len(),
        start.elapsed().as_millis()
    );
    Ok(())
}

/// Upsert validator metadata for the current epoch.
///
/// This writes validator data only. The BAM fields — `running_bam` and the connection
/// counters (`bam_total_snapshots` / `bam_connected_count`) — are owned by the separate BAM
/// snapshot job ([`write_bam_snapshot`]), which runs on its own (coarser) cadence. They are
/// therefore excluded from `$set` so a validator write never clobbers them, and seeded via
/// `$setOnInsert` so the fields exist as soon as a new epoch document is created (epoch
/// rollover) rather than being absent until the first snapshot lands.
pub async fn upsert_to_db(
    collection: &Collection<Validator>,
    items: &[Validator],
    epoch: u64,
) -> Result<()> {
    let start = Instant::now();
    let batch_size = 100;

    let update_options = UpdateOptions::builder().upsert(true).build();

    for (i, chunk) in items.chunks(batch_size).enumerate() {
        info!(
            "Processing batch {} of {}",
            i + 1,
            items.len().div_ceil(batch_size)
        );

        for item in chunk {
            let mut set_doc = bson::to_document(item).map_err(|e| AppError::from(e.to_string()))?;
            // The BAM snapshot job owns these fields; never overwrite them from a validator write.
            set_doc.remove("running_bam");
            set_doc.remove("bam_connected_count");
            set_doc.remove("bam_total_snapshots");

            collection
                .update_one(
                    doc! {
                        "epoch": epoch as u32,
                        "vote_account": &item.vote_account
                    },
                    doc! {
                        "$set": set_doc,
                        "$setOnInsert": {
                            "running_bam": Bson::Null,
                            "bam_connected_count": 0_i32,
                            "bam_total_snapshots": 0_i32
                        },
                    },
                    update_options.clone(),
                )
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

/// Record one BAM connection snapshot for the current epoch.
///
/// Runs on its own cadence, decoupled from the validator write. `connected` carries the BAM
/// API result: `None` means the API was not queried (not configured), so the snapshot is
/// skipped entirely and the denominator is left alone. `Some(set)` means the API was queried
/// successfully — the snapshot is recorded even when the set is empty, because a valid
/// zero-connected response is a real observation, not a reason to bias the denominator.
/// (Query *failures* never reach here: they surface as an `Err` from `fetch_bam_connected_set`
/// and the caller skips the snapshot.)
///
/// For every validator with a document this epoch we bump `bam_total_snapshots` (the
/// denominator) and default `running_bam` to `false`; validators in `connected` then get
/// `bam_connected_count` bumped and `running_bam` set to `true`. Validators without a document
/// yet this epoch (not written by the validator job) are simply skipped this snapshot.
pub async fn write_bam_snapshot(
    collection: &Collection<Validator>,
    epoch: u64,
    connected: Option<&HashSet<String>>,
) -> Result<()> {
    let Some(connected) = connected else {
        info!("BAM API not configured; skipping BAM snapshot for epoch {epoch}");
        return Ok(());
    };

    let start = Instant::now();

    // Denominator: every validator observed this epoch counts as one snapshot and defaults to
    // not-connected until the numerator pass below proves otherwise.
    collection
        .update_many(
            doc! { "epoch": epoch as u32 },
            doc! {
                "$inc": { "bam_total_snapshots": 1_i32 },
                "$set": { "running_bam": false },
            },
            None,
        )
        .await?;

    // Numerator: validators the BAM API reports as connected this snapshot. The connected set
    // can be large, so chunk the `$in` filter into batches rather than issuing one oversized
    // query, mirroring the batching `upsert_to_db` uses.
    let batch_size = 100;
    let connected_ids: Vec<&str> = connected.iter().map(String::as_str).collect();
    let mut marked_connected = 0_u64;

    for chunk in connected_ids.chunks(batch_size) {
        let result = collection
            .update_many(
                doc! {
                    "epoch": epoch as u32,
                    "identity_account": { "$in": chunk }
                },
                doc! {
                    "$inc": { "bam_connected_count": 1_i32 },
                    "$set": { "running_bam": true },
                },
                None,
            )
            .await?;
        marked_connected += result.modified_count;

        // Small delay between batches to avoid overwhelming the server
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    info!(
        "BAM snapshot for epoch {epoch}: marked {marked_connected} connected (BAM reported {}), took {}ms",
        connected.len(),
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
        .find_one(doc! {"epoch": (target_epoch) as u32}, None)
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

/// Query the BAM API for the currently-connected validator set and record one snapshot for
/// `epoch`. See [`write_bam_snapshot`] for the snapshot semantics.
pub async fn write_bam_snapshot_info(
    db: &Database,
    stake_pool_manager: &StakePoolManager,
    epoch: u64,
) -> Result<()> {
    let connected = stake_pool_manager.fetch_bam_connected_set().await?;
    let collection = db.collection::<Validator>(VALIDATOR_COLLECTION_NAME);
    write_bam_snapshot(&collection, epoch, connected.as_ref()).await
}

pub async fn setup_mongo_client(uri: &str) -> Result<MongodbClient> {
    let client_options = ClientOptions::parse(uri).await?;
    Ok(MongodbClient::with_options(client_options)?)
}
