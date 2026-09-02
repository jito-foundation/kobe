//! Checks that the steward writer can still read the transaction format the
//! cluster is producing right now.
//!
//! The writer asks the RPC for `Json`-encoded transactions with
//! `maxSupportedTransactionVersion: 1` and reads the fee payer out of the
//! node's parsed message, rather than deserializing the message itself. That
//! change is what makes v1 (SIMD-0296 / SIMD-0385) transactions readable on the
//! pinned 2.3.x SDK, but it also has to keep working for the legacy and v0
//! transactions that are all the cluster produces today. Unit tests cover both
//! against synthetic payloads; this samples real ones.
//!
//! ```text
//! cargo run -p kobe-steward-writer-service --example verify_transaction_format -- \
//!     --rpc-url https://api.mainnet-beta.solana.com
//! ```
//!
//! Exits non-zero if any sampled transaction could not be read, so it doubles
//! as a smoke check to re-run once v1 activates on a cluster.

use std::{collections::BTreeMap, process::ExitCode, str::FromStr};

use clap::Parser;
use kobe_core::rpc_utils::retry_get_transactions;
use kobe_steward_writer_service::{describe_version, fee_payer, get_epoch_from_slot, parse_log};
use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedConfirmedTransactionWithStatusMeta,
};

#[derive(Parser)]
#[command(about = "Check the steward writer can read the cluster's current transaction format")]
struct Args {
    /// RPC URL to check against.
    #[arg(long)]
    rpc_url: String,

    /// Program whose recent transactions to sample. Defaults to the steward program.
    #[arg(long)]
    program_id: Option<Pubkey>,

    /// How many recent transactions to sample.
    #[arg(long, default_value_t = 20)]
    limit: usize,

    /// Check these signatures instead of sampling. Repeatable.
    #[arg(long = "signature")]
    signatures: Vec<Signature>,

    /// Page back from this signature rather than from the most recent.
    #[arg(long)]
    before: Option<Signature>,
}

#[tokio::main]
async fn main() -> ExitCode {
    env_logger::init();
    let args = Args::parse();

    let program_id = args.program_id.unwrap_or_else(jito_steward::id);
    let rpc_client = RpcClient::new_with_commitment(args.rpc_url, CommitmentConfig::confirmed());

    let signatures = if args.signatures.is_empty() {
        println!(
            "Sampling up to {} transactions for {program_id}\n",
            args.limit
        );

        let statuses = match rpc_client
            .get_signatures_for_address_with_config(
                &program_id,
                GetConfirmedSignaturesForAddress2Config {
                    before: args.before,
                    until: None,
                    limit: Some(args.limit),
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await
        {
            Ok(statuses) => statuses,
            Err(e) => {
                eprintln!("Failed to fetch signatures: {e}");
                return ExitCode::FAILURE;
            }
        };

        if statuses.is_empty() {
            println!("No recent transactions for {program_id}; nothing to check.");
            return ExitCode::SUCCESS;
        }

        statuses
            .iter()
            .map(|status| {
                Signature::from_str(&status.signature).expect("RPC returned a valid signature")
            })
            .collect()
    } else {
        println!("Checking {} given signatures\n", args.signatures.len());
        args.signatures.clone()
    };

    // The writer's own fetch path, so whatever encoding and
    // `maxSupportedTransactionVersion` it requests, this requests too.
    let transactions = match retry_get_transactions(&rpc_client, &signatures).await {
        Ok(transactions) => transactions,
        Err(e) => {
            eprintln!("Failed to fetch transactions: {e}");
            eprintln!(
                "An UnsupportedTransactionVersion error here means the cluster produces a \
                 version above MAX_SUPPORTED_TRANSACTION_VERSION in kobe_core::rpc_utils."
            );
            return ExitCode::FAILURE;
        }
    };

    let mut readable_by_version: BTreeMap<String, usize> = BTreeMap::new();
    let mut decoded_by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut unreadable = Vec::new();
    let mut total_event_logs = 0;
    let mut undecodable_event_logs = 0;

    for (signature, tx) in &transactions {
        let version = describe_version(tx.transaction.version.as_ref());
        let (logs, event_logs) = log_counts(tx);
        let instruction = instruction_name(tx);
        total_event_logs += event_logs;

        let events = decoded_events(signature, tx).await;
        for event in &events {
            match event {
                Some(name) => *decoded_by_type.entry(name.clone()).or_default() += 1,
                None => undecodable_event_logs += 1,
            }
        }

        match fee_payer(&tx.transaction.transaction) {
            Some(payer) => {
                *readable_by_version.entry(version.clone()).or_default() += 1;
                println!(
                    "  ok    {version:>7}  {instruction:<18}  logs {logs:>3}  events {event_logs:>3}  payer {payer}"
                );
            }
            None => {
                println!(
                    "  FAIL  {version:>7}  {instruction:<18}  no fee payer readable  {signature}"
                );
                unreadable.push((*signature, version));
            }
        }

        if !events.is_empty() {
            let names: Vec<&str> = events
                .iter()
                .map(|e| e.as_deref().unwrap_or("UNDECODABLE"))
                .collect();
            println!("          decoded: {}", names.join(", "));
        }
    }

    println!("\nReadable by version:");
    for (version, count) in &readable_by_version {
        println!("  {version:>7}: {count}");
    }

    println!("Anchor event log lines seen: {total_event_logs}");
    if total_event_logs == 0 {
        println!(
            "  (0 is expected for `Idle` cranks, which emit no events. Pass --signature with a \
             `Rebalance` or `ComputeScore` transaction to exercise the log path.)"
        );
    } else {
        println!("Decoded steward events:");
        for (name, count) in &decoded_by_type {
            println!("  {name:<28}: {count}");
        }
        if undecodable_event_logs > 0 {
            println!(
                "  {:<28}: {undecodable_event_logs}  <-- writer would store nothing for these",
                "UNDECODABLE"
            );
        }
    }

    if unreadable.is_empty() && undecodable_event_logs == 0 {
        println!(
            "\nAll {} sampled transactions readable, all {total_event_logs} event logs decoded. \
             The writer can read this cluster's format.",
            transactions.len()
        );
        return ExitCode::SUCCESS;
    }

    if !unreadable.is_empty() {
        println!(
            "\n{} of {} transactions unreadable:",
            unreadable.len(),
            transactions.len()
        );
        for (signature, version) in &unreadable {
            println!("  {version} {signature}");
        }
        println!(
            "The writer skips these and emits steward_writer_service-unreadable_transaction \
             for each, so their events are lost. Check the encoding requested in \
             kobe_core::rpc_utils::transaction_config."
        );
    }

    if undecodable_event_logs > 0 {
        println!(
            "\n{undecodable_event_logs} event log(s) matched no event type the writer knows. \
             That is silent loss: parse_log returns Ok(None) and nothing is stored or logged. \
             Likely an on-chain event type newer than the pinned jito-steward rev."
        );
    }

    ExitCode::FAILURE
}

/// The instruction Anchor logged, so a zero event count can be read in context
/// — `Idle` emitting nothing is correct, `Rebalance` emitting nothing is not.
fn instruction_name(tx: &EncodedConfirmedTransactionWithStatusMeta) -> String {
    const PREFIX: &str = "Program log: Instruction: ";

    match tx.transaction.meta.as_ref().map(|meta| &meta.log_messages) {
        Some(OptionSerializer::Some(logs)) => logs
            .iter()
            .find_map(|log| log.strip_prefix(PREFIX))
            .unwrap_or("?")
            .to_string(),
        _ => "?".to_string(),
    }
}

/// Total log lines and the `Program data:` subset Anchor emits events as.
///
/// The writer's `parse_log` consumes these. They come from `meta`, not the
/// encoded message, so they are unaffected by the encoding — reporting them
/// just confirms the event pipeline still has its input.
fn log_counts(tx: &EncodedConfirmedTransactionWithStatusMeta) -> (usize, usize) {
    match tx.transaction.meta.as_ref().map(|meta| &meta.log_messages) {
        Some(OptionSerializer::Some(logs)) => (
            logs.len(),
            logs.iter()
                .filter(|log| log.starts_with("Program data:"))
                .count(),
        ),
        _ => (0, 0),
    }
}

/// Every `Program data:` line paired with the `event_type` the writer's own
/// `parse_log` decodes it to, or `None` when it recognises nothing.
///
/// This calls the production decoder rather than reimplementing its dispatch,
/// so the names printed are exactly what would land in Mongo. A `None` is worth
/// surfacing: `parse_log` returns `Ok(None)` and the writer stores nothing and
/// logs nothing, which is silent loss rather than a visible failure.
async fn decoded_events(
    signature: &Signature,
    tx: &EncodedConfirmedTransactionWithStatusMeta,
) -> Vec<Option<String>> {
    let Some(meta) = tx.transaction.meta.as_ref() else {
        return vec![];
    };
    let OptionSerializer::Some(logs) = &meta.log_messages else {
        return vec![];
    };

    // `parse_log` only uses the stake pool to label the event, and this checks
    // decodability rather than event contents.
    let stake_pool = Pubkey::default();
    let signer = fee_payer(&tx.transaction.transaction).unwrap_or_default();

    let mut decoded = Vec::new();
    for log in logs.iter().filter(|log| log.starts_with("Program data:")) {
        let event = parse_log(
            log.clone(),
            signature,
            0,
            &signer,
            &stake_pool,
            tx.block_time,
            meta.err.clone(),
            get_epoch_from_slot(tx.slot),
            tx.slot,
        )
        .await;

        decoded.push(match event {
            Ok(Some(event)) => Some(event.event_type),
            Ok(None) => None,
            Err(e) => {
                eprintln!("  parse_log errored on {signature}: {e}");
                None
            }
        });
    }
    decoded
}
