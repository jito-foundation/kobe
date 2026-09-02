use std::{str::FromStr, time::Duration};

use clap::{Parser, Subcommand};
use kobe_core::db_models::steward_events::{StewardEvent, StewardEventsStore};
use kobe_core::rpc_utils::{retry_get_slot, retry_get_transactions};
use kobe_steward_writer_service::{
    describe_version, fee_payer, get_epoch_from_slot, pair_with_statuses, parse_log,
};
use log::{debug, error, info};
use mongodb::{Client, Collection};
use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_response::RpcConfirmedTransactionStatusWithSignature,
};
use solana_metrics::{datapoint_error, datapoint_info};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Commands,

    /// Mongo connection URI.
    #[clap(long, env)]
    mongo_connection_uri: String,

    /// Mongo database name.
    #[clap(long, env)]
    mongo_db_name: String,

    /// RPC URL.
    #[clap(long, env)]
    rpc_url: String,

    /// Program ID
    #[clap(long, env)]
    program_id: Pubkey,

    /// Stake pool address
    #[clap(long, env)]
    stake_pool: Pubkey,

    /// Whether to dry run before writing to db
    #[clap(long, env, action)]
    dry_run: bool,

    /// Cluster name for metrics
    #[clap(long, env, default_value = "mainnet")]
    cluster_name: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Listen for new events
    Listen,
    /// Backfill events from a specific slot range
    Backfill {
        /// Start slot for backfilling
        #[clap(long)]
        start_slot: u64,
        /// End slot for backfilling (optional)
        #[clap(long)]
        end_slot: Option<u64>,
    },
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    // Connect to MongoDB
    let client = Client::with_uri_str(&args.mongo_connection_uri)
        .await
        .expect("Failed to connect to MongoDB");
    let db = client.database(&args.mongo_db_name);

    let events_collection: Collection<StewardEvent> = db.collection(StewardEventsStore::COLLECTION);
    let store = StewardEventsStore::new(events_collection);

    // Connect to RPC node
    let client = RpcClient::new_with_timeout_and_commitment(
        args.rpc_url,
        Duration::from_secs(20),
        CommitmentConfig::finalized(),
    );

    match args.command {
        Commands::Listen => {
            info!("Listening for new events");
            let polling_duration = Duration::from_secs(300); // Configurable polling duration (5 mins)
            loop {
                if let Err(e) = listen(
                    &args.program_id,
                    &args.stake_pool,
                    &store,
                    &client,
                    polling_duration,
                    args.dry_run,
                    &args.cluster_name,
                )
                .await
                {
                    error!("Error in listen loop: {e:?}");
                }
            }
        }
        Commands::Backfill {
            start_slot,
            end_slot,
        } => {
            info!("Backfilling events from slot {start_slot} to {end_slot:?}");
            let end_slot = if let Some(end_slot) = end_slot {
                end_slot
            } else {
                match client.get_epoch_info().await {
                    Ok(epoch_info) => epoch_info.absolute_slot,
                    Err(e) => {
                        info!("Error: {e:?}");
                        return;
                    }
                }
            };

            // Implement backfilling logic here
            if let Err(e) = fetch_historical_program_transactions(
                &args.program_id,
                &client,
                &args.stake_pool,
                &store,
                start_slot,
                end_slot,
                args.dry_run,
                &args.cluster_name,
            )
            .await
            {
                info!("Error: {e:?}");
            }
        }
    }
}

async fn listen(
    program_id: &Pubkey,
    stake_pool: &Pubkey,
    store: &StewardEventsStore,
    rpc_client: &RpcClient,
    polling_duration: Duration,
    dry_run: bool,
    cluster_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut interval = tokio::time::interval(polling_duration);

    loop {
        interval.tick().await;

        let (latest_signature, slot) = match store.get_latest_signature_and_slot().await? {
            Some((sig, slot)) => (sig, slot),
            None => {
                info!("No existing slot found..");
                continue;
            }
        };
        let cluster_slot = retry_get_slot(rpc_client).await?;

        datapoint_info!(
            "steward_writer_service-slot",
            ("slot", slot, i64),
            ("cluster_slot", cluster_slot, i64),
            "cluster" => cluster_name,
        );

        info!("Fetching new transactions since signature: {latest_signature}");

        let mut before = None;

        loop {
            debug!("before: {before:?}, latest_signature: {latest_signature}");

            let rpc_signatures_res = match rpc_client
                .get_signatures_for_address_with_config(
                    program_id,
                    GetConfirmedSignaturesForAddress2Config {
                        before,
                        until: Some(latest_signature),
                        limit: Some(NUM_TRANSACTIONS),
                        commitment: Some(CommitmentConfig::confirmed()),
                    },
                )
                .await
            {
                Ok(signatures) => signatures,
                Err(e) => {
                    info!("Error fetching RPC signatures: {e}");
                    continue;
                }
            };

            let rpc_signatures = rpc_signatures_res.into_iter().rev().collect::<Vec<_>>();

            if rpc_signatures.is_empty() {
                break;
            }

            before = rpc_signatures
                .first()
                .map(|status| Signature::from_str(&status.signature).unwrap());

            fetch_and_process_transactions(
                rpc_client,
                &rpc_signatures,
                stake_pool,
                store,
                dry_run,
                cluster_name,
            )
            .await?;
        }
    }
}

async fn fetch_and_process_transactions(
    rpc_client: &RpcClient,
    signatures: &[RpcConfirmedTransactionStatusWithSignature],
    stake_pool: &Pubkey,
    store: &StewardEventsStore,
    dry_run: bool,
    cluster_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let transaction_signatures: Vec<Signature> = signatures
        .iter()
        .map(|status| Signature::from_str(&status.signature).unwrap())
        .collect();

    let transactions = retry_get_transactions(rpc_client, &transaction_signatures).await?;

    info!("Fetched {} transactions from rpc", transactions.len());

    let transaction_data = pair_with_statuses(signatures, transactions);

    process_transactions(&transaction_data, stake_pool, store, dry_run, cluster_name).await
}

async fn process_transactions(
    transactions: &[(
        RpcConfirmedTransactionStatusWithSignature,
        EncodedConfirmedTransactionWithStatusMeta,
    )],
    stake_pool: &Pubkey,
    store: &StewardEventsStore,
    dry_run: bool,
    cluster_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // If the slot from `signatures` doesn't match the slot in `transactions`, print it out
    for (status, tx) in transactions.iter() {
        if tx.slot != status.slot {
            error!(
                "Slot mismatch for signature {}: {} != {}",
                status.signature, tx.slot, status.slot
            );
        }
    }

    let mut events = Vec::new();
    for (status, encoded_tx_with_meta) in transactions {
        let RpcConfirmedTransactionStatusWithSignature {
            signature,
            slot,
            err,
            memo: _,
            block_time,
            confirmation_status: _,
        } = status;

        let signature = Signature::from_str(signature).unwrap();

        let EncodedConfirmedTransactionWithStatusMeta { transaction, .. } = encoded_tx_with_meta;

        let signer: Pubkey = match fee_payer(&transaction.transaction) {
            Some(signer) => signer,
            None => {
                let version = describe_version(transaction.version.as_ref());
                error!("No fee payer in {version} transaction {signature}, skipping its events");
                datapoint_error!(
                    "steward_writer_service-unreadable_transaction",
                    ("signature", signature.to_string(), String),
                    ("version", version, String),
                    ("slot", *slot as i64, i64),
                    "cluster" => cluster_name,
                );
                continue;
            }
        };

        let epoch = get_epoch_from_slot(*slot);
        let instruction_idx = 0;

        // Process logs
        if let Some(meta) = &encoded_tx_with_meta.transaction.meta {
            if let OptionSerializer::Some(log_messages) = meta.log_messages.clone() {
                for log in log_messages.into_iter() {
                    match parse_log(
                        log,
                        &signature,
                        instruction_idx as u32,
                        &signer,
                        stake_pool,
                        *block_time,
                        err.clone(),
                        epoch,
                        *slot,
                    )
                    .await
                    {
                        Ok(Some(event)) => events.push(event),
                        Ok(None) => {}
                        Err(e) => error!(
                            "Error parsing log message for transaction {:?}: {:?}",
                            signature,
                            e.to_string()
                        ),
                    }
                }
            }
        }
    }

    match dry_run {
        true => {
            info!("upserting {events:#?}");
        }
        false => {
            if let Err(e) = store.bulk_upsert(events).await {
                error!("Error inserting events: {e:?}");
            }
        }
    }

    Ok(())
}

const NUM_TRANSACTIONS: usize = 1000;

#[allow(clippy::too_many_arguments)]
async fn fetch_historical_program_transactions(
    program_id: &Pubkey,
    rpc_client: &RpcClient,
    stake_pool: &Pubkey,
    store: &StewardEventsStore,
    start_slot: u64,
    end_slot: u64,
    dry_run: bool,
    cluster_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Backfilling transactions between slots {start_slot} and {end_slot}");
    let mut before = None;
    let mut should_break = false;

    loop {
        let signatures = match rpc_client
            .get_signatures_for_address_with_config(
                program_id,
                GetConfirmedSignaturesForAddress2Config {
                    before,
                    until: None,
                    limit: Some(NUM_TRANSACTIONS),
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await
        {
            Ok(signatures) => signatures,
            Err(e) => {
                info!("Error fetching RPC signatures: {e}");
                continue;
            }
        };

        // Get signatures in chronological order
        let signatures_vec = signatures.into_iter().collect::<Vec<_>>();

        // Set before to the oldest signature for next iteration
        before = signatures_vec
            .last()
            .map(|status| Signature::from_str(&status.signature).unwrap());

        // Filter out signatures before start_slot and check if we should break after processing
        let valid_signatures: Vec<RpcConfirmedTransactionStatusWithSignature> = signatures_vec
            .into_iter()
            .filter_map(|status| {
                if status.slot < start_slot {
                    should_break = true;
                    None
                } else if status.slot <= end_slot {
                    Some(status)
                } else {
                    None
                }
            })
            .rev() // We still want to process in chronological order
            .collect::<Vec<_>>();

        if valid_signatures.is_empty() {
            continue;
        }

        info!(
            "Processing {} transactions starting at slot {}",
            valid_signatures.len(),
            valid_signatures[0].slot
        );

        let transaction_signatures: Vec<Signature> = valid_signatures
            .iter()
            .map(|status| Signature::from_str(&status.signature).unwrap())
            .collect();

        let transactions = retry_get_transactions(rpc_client, &transaction_signatures).await?;

        let transaction_data = pair_with_statuses(&valid_signatures, transactions);

        process_transactions(&transaction_data, stake_pool, store, dry_run, cluster_name).await?;

        if should_break {
            break;
        }
    }
    Ok(())
}

