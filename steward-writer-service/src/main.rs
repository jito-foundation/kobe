use std::{str::FromStr, time::Duration};

use anchor_client::handle_program_log;
use clap::{Parser, Subcommand};
use jito_steward::{
    events::{
        AutoAddValidatorEvent, AutoRemoveValidatorEvent, DecreaseComponents,
        DirectedRebalanceEvent, EpochMaintenanceEvent, InstantUnstakeComponents, RebalanceEvent,
        ScoreComponents, StateTransition,
    },
    score::{InstantUnstakeComponentsV3, ScoreComponentsV5},
};
use kobe_core::db_models::steward_events::{StewardEvent, StewardEventsStore};
use kobe_core::rpc_utils::{retry_get_slot, retry_get_transactions};
use log::{debug, error, info};
use mongodb::{Client, Collection};
use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_response::RpcConfirmedTransactionStatusWithSignature,
};
use solana_metrics::{datapoint_error, datapoint_info};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Signature,
    transaction::{TransactionError, TransactionVersion},
};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
    EncodedTransaction, UiMessage,
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

/// Pairs fetched transactions back up with the signature statuses they came from.
///
/// `retry_get_transactions` returns one entry per requested signature in request
/// order, so this is a positional zip. The equality check guards against
/// attributing a transaction's events to the wrong signature if that ever stops
/// holding.
fn pair_with_statuses(
    signatures: &[RpcConfirmedTransactionStatusWithSignature],
    transactions: Vec<(Signature, EncodedConfirmedTransactionWithStatusMeta)>,
) -> Vec<(
    RpcConfirmedTransactionStatusWithSignature,
    EncodedConfirmedTransactionWithStatusMeta,
)> {
    signatures
        .iter()
        .zip(transactions)
        .filter_map(|(status, (signature, tx))| {
            if status.signature != signature.to_string() {
                error!(
                    "Signature mismatch: requested {}, RPC returned {signature}",
                    status.signature
                );
                return None;
            }
            Some((status.clone(), tx))
        })
        .collect()
}

/// Human-readable transaction version, for diagnostics.
fn describe_version(version: Option<&TransactionVersion>) -> String {
    match version {
        Some(TransactionVersion::Number(n)) => format!("v{n}"),
        Some(TransactionVersion::Legacy(_)) => "legacy".to_string(),
        None => "unknown-version".to_string(),
    }
}

/// Reads the fee payer out of an RPC-parsed transaction message.
///
/// The first account key is the fee payer in every transaction version. Taking
/// it from the node's parsed output rather than deserializing the message
/// ourselves is what lets this keep working for versions the pinned SDK has no
/// decoder for — see the encoding choice in `kobe_core::rpc_utils`.
fn fee_payer(transaction: &EncodedTransaction) -> Option<Pubkey> {
    let first_key = match transaction {
        EncodedTransaction::Json(ui_transaction) => match &ui_transaction.message {
            UiMessage::Raw(message) => message.account_keys.first().cloned(),
            UiMessage::Parsed(message) => {
                message.account_keys.first().map(|key| key.pubkey.clone())
            }
        },
        // A binary encoding was requested somewhere; we can't parse it here
        // without a version-aware decoder.
        EncodedTransaction::LegacyBinary(_)
        | EncodedTransaction::Binary(_, _)
        | EncodedTransaction::Accounts(_) => None,
    }?;

    Pubkey::from_str(&first_key).ok()
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

#[allow(clippy::too_many_arguments)]
async fn parse_log(
    log: String,
    signature: &Signature,
    instruction_idx: u32,
    signer: &Pubkey,
    stake_pool: &Pubkey,
    timestamp: Option<i64>,
    transaction_err: Option<TransactionError>,
    epoch: u64,
    slot: u64,
) -> Result<Option<StewardEvent>, Box<dyn std::error::Error>> {
    // Parse the log
    let program = jito_steward::id().to_string();
    let tx_error = transaction_err.map(|e| e.to_string());

    // DecreaseComponents
    if let Ok((Some(event), _, _)) = handle_program_log::<DecreaseComponents>(&program, &log) {
        let steward_event = StewardEvent::from_decrease_components(
            event,
            signature,
            instruction_idx,
            tx_error,
            epoch,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // InstantUnstakeComponents
    if let Ok((Some(event), _, _)) = handle_program_log::<InstantUnstakeComponents>(&program, &log)
    {
        let steward_event = StewardEvent::from_instant_unstake_components(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // InstantUnstakeComponentsV3
    if let Ok((Some(event), _, _)) =
        handle_program_log::<InstantUnstakeComponentsV3>(&program, &log)
    {
        let steward_event = StewardEvent::from_instant_unstake_components_v3(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // RebalanceEvent
    if let Ok((Some(event), _, _)) = handle_program_log::<RebalanceEvent>(&program, &log) {
        let steward_event = StewardEvent::from_rebalance_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // DirectedRebalanceEvent
    if let Ok((Some(event), _, _)) = handle_program_log::<DirectedRebalanceEvent>(&program, &log) {
        let steward_event = StewardEvent::from_directed_rebalance_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // ScoreComponents
    if let Ok((Some(event), _, _)) = handle_program_log::<ScoreComponents>(&program, &log) {
        let steward_event = StewardEvent::from_score_components(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // ScoreComponentsV5
    if let Ok((Some(event), _, _)) = handle_program_log::<ScoreComponentsV5>(&program, &log) {
        let steward_event = StewardEvent::from_score_components_v5(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // StateTransition
    if let Ok((Some(event), _, _)) = handle_program_log::<StateTransition>(&program, &log) {
        let steward_event = StewardEvent::from_state_transition(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // AutoRemoveValidatorEvent
    if let Ok((Some(event), _, _)) =
        handle_program_log::<AutoRemoveValidatorEvent>(&program.to_string(), &log)
    {
        let steward_event = StewardEvent::from_auto_remove_validator_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            epoch,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // AutoAddValidatorEvent
    if let Ok((Some(event), _, _)) =
        handle_program_log::<AutoAddValidatorEvent>(&program.to_string(), &log)
    {
        let steward_event = StewardEvent::from_auto_add_validator_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            epoch,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // EpochMaintenanceEvent
    if let Ok((Some(event), _, _)) =
        handle_program_log::<EpochMaintenanceEvent>(&program.to_string(), &log)
    {
        let steward_event = StewardEvent::from_epoch_maintenance_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            epoch,
            slot,
        );
        return Ok(Some(steward_event));
    }

    Ok(None)
}

fn get_epoch_from_slot(slot: u64) -> u64 {
    // Calculate the epoch from the slot

    slot / 432_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_transaction_status::EncodedTransactionWithStatusMeta;

    const FEE_PAYER: &str = "6WS1UtWtyeJHrsGbcARDPuLoXQeQTuFJHnsL2h1yc9CB";
    const OTHER_KEY: &str = "SysvarC1ock11111111111111111111111111111111";

    /// A `getTransaction` response as an Agave v4.2 node renders a v1
    /// transaction: `version: 1`, and a `transactionConfig` field on the
    /// message that this SDK's `UiRawMessage` has no field for.
    fn v1_transaction_json() -> String {
        format!(
            r#"{{
                "transaction": {{
                    "signatures": ["4HVYbFHkGwPjsHo3jNTfBnhJKzHrMg1S5g8vCsWpMHQFmDGKcNEqzP2M8j1YFwGZJnVw8UzKRZKtQPk2ZTgNYQwr"],
                    "message": {{
                        "header": {{
                            "numRequiredSignatures": 1,
                            "numReadonlySignedAccounts": 0,
                            "numReadonlyUnsignedAccounts": 1
                        }},
                        "accountKeys": ["{FEE_PAYER}", "{OTHER_KEY}"],
                        "recentBlockhash": "EkSnNWid2cvwEVnVx9aBqawnmiCNiDgp3gUdkDPTKN1N",
                        "instructions": [
                            {{"programIdIndex": 1, "accounts": [0], "data": "3Bxs"}}
                        ],
                        "transactionConfig": {{
                            "computeUnitLimit": 200000,
                            "loadedAccountsDataSizeLimit": 65536,
                            "feeLamports": 5000
                        }}
                    }}
                }},
                "meta": {{
                    "err": null,
                    "status": {{"Ok": null}},
                    "fee": 5000,
                    "preBalances": [1000000, 0],
                    "postBalances": [995000, 0],
                    "logMessages": ["Program log: hello"]
                }},
                "version": 1
            }}"#
        )
    }

    /// The whole v1 mitigation rests on this: an unknown `transactionConfig`
    /// field must not fail deserialization, or every v1 transaction becomes an
    /// RPC error rather than a readable record.
    #[test]
    fn v1_response_deserializes_under_pinned_sdk() {
        let encoded: EncodedTransactionWithStatusMeta =
            serde_json::from_str(&v1_transaction_json()).expect("v1 response should deserialize");

        assert_eq!(encoded.version, Some(TransactionVersion::Number(1)));
        assert_eq!(describe_version(encoded.version.as_ref()), "v1");
    }

    #[test]
    fn fee_payer_reads_first_account_key_of_v1_transaction() {
        let encoded: EncodedTransactionWithStatusMeta =
            serde_json::from_str(&v1_transaction_json()).unwrap();

        assert_eq!(
            fee_payer(&encoded.transaction),
            Some(Pubkey::from_str(FEE_PAYER).unwrap())
        );
    }

    /// A binary encoding yields no fee payer here by design: parsing it needs a
    /// version-aware `bincode` decoder, which is the thing this SDK lacks. The
    /// matching guard on the request side lives in `kobe_core::rpc_utils`.
    #[test]
    fn fee_payer_rejects_binary_encoding() {
        let binary = EncodedTransaction::LegacyBinary("not parseable here".to_string());

        assert_eq!(fee_payer(&binary), None);
    }

    /// The five Anchor event logs emitted by a real `ComputeScore` transaction,
    /// mainnet signature
    /// `3AaEZ6PUYUmNH8tFbx3tHUs6buJhY3KhhNtri6U1Rm4xGo17ct2M6nVyCEdXqn7DoVjakgYBwoaAAg9EDAY5PmjB`
    /// at slot 440863635. Verbatim, so this covers the real on-chain event
    /// layout rather than one we constructed to match the decoder.
    const COMPUTE_SCORE_EVENT_LOGS: [&str; 5] = [
        "Program data: HqAPeIvXzYIAAAAAAAAAAK//PBgAoIxfBegDDAAAAK//PAABAQEAAAEBAQaIjdQdpdfhgZtLvBcd/40lJ0Xw87GYrNLoZWOCWwnL/APoA/AD//8AAAAAAAAAAN4DBfADBfADAAD//wEB",
        "Program data: HqAPeIvXzYIAAAAAAAAAAI4APRgAoIxfBegDDAAAAI4APQABAQEAAQEBAfY3xpialume/Kul7cD7zTtHMU62qGkHLR3yAm8ydlrv/APoA/AD//8AAAAAAAAAAN4DBfADBfADAAD//wEB",
        "Program data: HqAPeIvXzYIAAAAAAAAAAF6lKBAAcJRfBfQBCAAAAF6lKAABAQEAAAEBAeQpUoVdDp1KFUpvB84QbrDAG6KlLw/zMKtIqUEytqoy/AP0AfQD//8AAAAAAAAAAN4DBfQDBfQDAAD//wEB",
        "Program data: HqAPeIvXzYIAAAAAAAAAALxiGQoAQJxfBQAABQAAALxiGQABAQEAAQEBAb7uYNbiR9EumO+5zbWeuSxnHpx9d/vAP8EO42iowV/1/AMAAPwD//8AAAAAAAAAAN4DBfcDBfcDAAD//wEB",
        "Program data: agl496lqzun8AwAAAAAAAJMLRxoAAAAADQAAAENvbXB1dGVTY29yZXMSAAAAQ29tcHV0ZURlbGVnYXRpb25z",
    ];

    /// Answers the question the encoding change raises end to end: a real
    /// `ComputeScore` transaction still decodes into the events the writer
    /// stores. `parse_log` reads `meta.log_messages`, which the `Json` encoding
    /// leaves untouched — this pins that.
    #[tokio::test]
    async fn decodes_events_from_real_compute_score_transaction() {
        let signature = Signature::from_str(
            "3AaEZ6PUYUmNH8tFbx3tHUs6buJhY3KhhNtri6U1Rm4xGo17ct2M6nVyCEdXqn7DoVjakgYBwoaAAg9EDAY5PmjB",
        )
        .unwrap();
        let signer = Pubkey::from_str("CRnkKQTxctQ7LHVN3yssdgJyEksBJeBrDdwZAxBtsJoZ").unwrap();
        let stake_pool = Pubkey::new_unique();
        let slot = 440_863_635;

        let mut events = Vec::new();
        for log in COMPUTE_SCORE_EVENT_LOGS {
            let parsed = parse_log(
                log.to_string(),
                &signature,
                0,
                &signer,
                &stake_pool,
                Some(1_787_380_325),
                None,
                get_epoch_from_slot(slot),
                slot,
            )
            .await
            .expect("parse_log should not error");

            if let Some(event) = parsed {
                events.push(event);
            }
        }

        let event_types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert_eq!(
            event_types,
            vec![
                "ScoreComponentsV5",
                "ScoreComponentsV5",
                "ScoreComponentsV5",
                "ScoreComponentsV5",
                "StateTransition",
            ],
        );

        // The score events carry the validator they scored; the state
        // transition is pool-wide and carries none.
        assert!(events[..4].iter().all(|e| e.vote_account.is_some()),);
        assert!(events.iter().all(|e| e.signer == signer.to_string()));
        assert!(events.iter().all(|e| e.slot == slot));
    }
}
