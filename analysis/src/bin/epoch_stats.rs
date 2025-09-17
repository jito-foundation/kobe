use std::fs::File;
use std::io::{self, Write};
use std::str::FromStr;

use clap::Parser;
use futures::TryStreamExt;
use kobe_core::constants::{STAKE_POOL_STATS_COLLECTION_NAME, STEWARD_EVENTS_COLLECTION_NAME};
use kobe_core::db_models::stake_pool_stats::StakePoolStats;
use kobe_core::db_models::steward_events::StewardEvent;
use log::{error, warn};
use mongodb::bson::{doc, Document};
use mongodb::options::{FindOneOptions, FindOptions};
use mongodb::{Client, Collection, Database};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{EncodedTransaction, UiMessage, UiTransactionEncoding};

#[derive(Parser, Debug, Clone)]
#[clap(author, version, about = "Export per-epoch stake pool stats as CSV")]
struct Args {
    /// Mongo connection URI.
    #[arg(long, env = "MONGO_CONNECTION_URI")]
    mongo_connection_uri: String,

    /// Mongo database name.
    #[arg(long, env = "MONGO_DB_NAME")]
    mongo_db_name: String,

    /// Solana RPC URL.
    #[arg(long, env = "RPC_URL")]
    rpc_url: String,

    /// Start epoch (inclusive).
    #[arg(long)]
    start_epoch: u64,

    /// End epoch (inclusive). If omitted, only start_epoch is exported.
    #[arg(long)]
    end_epoch: Option<u64>,

    /// Optional path to write CSV; defaults to stdout.
    #[arg(long)]
    out: Option<String>,

    /// Reserve account pubkey used to read postBalances from transactions.
    #[arg(long, default_value = "BgKUXdS29YcHCFrPm5M8oLHiTzZaMDjsebggjoaQ6KFL")]
    reserve_account: String,
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let end_epoch = args.end_epoch.unwrap_or(args.start_epoch);

    let mongo_client = Client::with_uri_str(&args.mongo_connection_uri)
        .await
        .expect("failed to connect to mongo");
    let database = mongo_client.database(&args.mongo_db_name);

    let events_collection: Collection<StewardEvent> =
        database.collection(STEWARD_EVENTS_COLLECTION_NAME);
    let stats_collection: Collection<StakePoolStats> =
        database.collection(STAKE_POOL_STATS_COLLECTION_NAME);

    let rpc_client = RpcClient::new(args.rpc_url.clone());

    let mut writer: Box<dyn Write> = match args.out.as_deref() {
        Some(path) => Box::new(File::create(path).expect("failed to create output file")),
        None => Box::new(io::stdout()),
    };

    writeln!(
        writer,
        "epoch,inc_lamports,inc_ratio,dec_lamports,dec_ratio,dec_scoring_lamports,dec_scoring_ratio,dec_instant_lamports,dec_instant_ratio,dec_stake_deposit_lamports,dec_stake_deposit_ratio,reserve_lamports,reserve_ratio,jitosol_lamports,jitosol_ratio,rebalance_ok_count,num_pool_validators"
    )
    .expect("failed to write header");

    let reserve_pubkey = Pubkey::from_str(&args.reserve_account).expect("invalid reserve pubkey");

    for epoch in args.start_epoch..=end_epoch {
        match export_epoch(
            epoch,
            &database,
            &events_collection,
            &stats_collection,
            &rpc_client,
            &reserve_pubkey,
        )
        .await
        {
            Ok(row) => {
                writeln!(writer, "{}", row).expect("failed to write csv row");
            }
            Err(e) => {
                error!("failed to export epoch {}: {}", epoch, e);
                // Still write a row with zeros so downstream tools can see the epoch
                writeln!(writer, "{epoch},0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0").ok();
            }
        }
    }
}

async fn export_epoch(
    epoch: u64,
    database: &Database,
    events_collection: &Collection<StewardEvent>,
    stats_collection: &Collection<StakePoolStats>,
    rpc_client: &RpcClient,
    reserve_pubkey: &Pubkey,
) -> Result<String, String> {
    let (inc, dec_total, dec_scoring, dec_instant, dec_stake_deposit) =
        sum_rebalance_components(events_collection, epoch).await?;

    let final_reserve_balance = find_final_reserve_balance(
        database,
        events_collection,
        rpc_client,
        reserve_pubkey,
        epoch,
    )
    .await
    .unwrap_or_else(|e| {
        warn!("epoch {epoch}: could not get final reserve balance: {}", e);
        0
    });

    let final_jitosol_balance = find_final_jitosol_balance(stats_collection, epoch)
        .await
        .unwrap_or_else(|e| {
            warn!("epoch {epoch}: could not get final jitosol balance: {}", e);
            0
        });

    let rebalance_ok_count = count_successful_rebalances(events_collection, epoch)
        .await
        .unwrap_or_else(|e| {
            warn!(
                "epoch {epoch}: could not count successful RebalanceEvents: {}",
                e
            );
            0
        });

    let num_pool_validators =
        find_latest_epoch_maintenance_num_pool_validators(events_collection, epoch)
            .await
            .unwrap_or_else(|e| {
                warn!(
            "epoch {epoch}: could not get latest EpochMaintenanceEvent num_pool_validators: {}",
            e
        );
                0
            });

    let denom = final_jitosol_balance as f64;
    let ratio = |n: f64| -> String {
        if final_jitosol_balance == 0 {
            "0".to_string()
        } else {
            format!("{:.6}", n / denom)
        }
    };

    let inc_r = ratio(inc as f64);
    let dec_total_r = ratio(dec_total as f64);
    let dec_scoring_r = ratio(dec_scoring as f64);
    let dec_instant_r = ratio(dec_instant as f64);
    let dec_stake_deposit_r = ratio(dec_stake_deposit as f64);
    let reserve_r = ratio(final_reserve_balance as f64);
    let jitosol_r = ratio(final_jitosol_balance as f64);

    Ok(format!(
        "{epoch},{inc},{inc_r},{dec_total},{dec_total_r},{dec_scoring},{dec_scoring_r},{dec_instant},{dec_instant_r},{dec_stake_deposit},{dec_stake_deposit_r},{final_reserve_balance},{reserve_r},{final_jitosol_balance},{jitosol_r},{rebalance_ok_count},{num_pool_validators}"
    ))
}

async fn sum_rebalance_components(
    events_collection: &Collection<StewardEvent>,
    epoch: u64,
) -> Result<(i64, i64, i64, i64, i64), String> {
    let filter = doc! { "epoch": epoch as i64, "event_type": "RebalanceEvent" };
    let mut cursor = events_collection
        .find(filter, None)
        .await
        .map_err(|e| e.to_string())?;

    let mut total_increase: i64 = 0;
    let mut total_decrease_total: i64 = 0;
    let mut total_scoring: i64 = 0;
    let mut total_instant: i64 = 0;
    let mut total_stake_deposit: i64 = 0;

    while let Some(event) = cursor
        .try_next()
        .await
        .map_err(|e| format!("cursor err: {}", e))?
    {
        if let Some(metadata) = event.metadata {
            if let Some(v) = get_i64(&metadata, "increase_lamports") {
                total_increase += v;
            }
            if let Ok(decr) = metadata.get_document("decrease_components") {
                if let Some(v) = get_i64(decr, "total_unstake_lamports") {
                    total_decrease_total += v;
                }
                if let Some(v) = get_i64(decr, "scoring_unstake_lamports") {
                    total_scoring += v;
                }
                if let Some(v) = get_i64(decr, "instant_unstake_lamports") {
                    total_instant += v;
                }
                if let Some(v) = get_i64(decr, "stake_deposit_unstake_lamports") {
                    total_stake_deposit += v;
                }
            }
        }
    }

    Ok((
        total_increase,
        total_decrease_total,
        total_scoring,
        total_instant,
        total_stake_deposit,
    ))
}

fn get_i64(doc: &Document, key: &str) -> Option<i64> {
    match doc.get(key) {
        Some(bson) => match bson {
            mongodb::bson::Bson::Int32(v) => Some(*v as i64),
            mongodb::bson::Bson::Int64(v) => Some(*v),
            mongodb::bson::Bson::Double(v) => Some(*v as i64),
            _ => None,
        },
        None => None,
    }
}

async fn find_final_reserve_balance(
    _database: &Database,
    events_collection: &Collection<StewardEvent>,
    rpc_client: &RpcClient,
    reserve_pubkey: &Pubkey,
    epoch: u64,
) -> Result<u64, String> {
    let find_opts = FindOptions::builder()
        .sort(doc! { "timestamp": -1 })
        .limit(50)
        .build();
    let mut cursor = events_collection
        .find(doc! { "epoch": epoch as i64 }, find_opts)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(event) = cursor
        .try_next()
        .await
        .map_err(|e| format!("cursor err: {}", e))?
    {
        let sig = Signature::from_str(&event.signature).map_err(|e| e.to_string())?;
        if let Ok(Some(balance)) =
            get_post_balance_for_account(rpc_client, &sig, reserve_pubkey).await
        {
            return Ok(balance);
        }
    }

    Err("no transaction in epoch contained the reserve account".to_string())
}

async fn get_post_balance_for_account(
    rpc_client: &RpcClient,
    signature: &Signature,
    account: &Pubkey,
) -> Result<Option<u64>, String> {
    let tx = rpc_client
        .get_transaction(signature, UiTransactionEncoding::Json)
        .await
        .map_err(|e| e.to_string())?;

    let meta = match tx.transaction.meta {
        Some(meta) => meta,
        None => return Ok(None),
    };

    let post_balances = meta.post_balances;

    let account_keys: Vec<String> = match tx.transaction.transaction {
        EncodedTransaction::Json(ui_tx) => match ui_tx.message {
            UiMessage::Raw(raw) => raw.account_keys,
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };

    if let Some(idx) = account_keys.iter().position(|k| k == &account.to_string()) {
        if let Some(balance) = post_balances.get(idx) {
            return Ok(Some(*balance));
        }
    }

    Ok(None)
}

async fn find_final_jitosol_balance(
    stats_collection: &Collection<StakePoolStats>,
    epoch: u64,
) -> Result<u64, String> {
    let filter = doc! { "epoch": epoch as i64 };
    let opts = FindOneOptions::builder()
        .sort(doc! { "timestamp": -1 })
        .build();
    let doc = stats_collection
        .find_one(filter, opts)
        .await
        .map_err(|e| e.to_string())?;
    Ok(doc.map(|d| d.total_solana_lamports).unwrap_or(0))
}

async fn count_successful_rebalances(
    events_collection: &Collection<StewardEvent>,
    epoch: u64,
) -> Result<u64, String> {
    let filter = doc! { "epoch": epoch as i64, "event_type": "RebalanceEvent", "tx_error": mongodb::bson::Bson::Null };
    events_collection
        .count_documents(filter, None)
        .await
        .map_err(|e| e.to_string())
}

async fn find_latest_epoch_maintenance_num_pool_validators(
    events_collection: &Collection<StewardEvent>,
    epoch: u64,
) -> Result<u64, String> {
    let filter = doc! { "epoch": epoch as i64, "event_type": "EpochMaintenanceEvent" };
    let opts = FindOneOptions::builder().sort(doc! { "slot": -1 }).build();
    let doc_opt = events_collection
        .find_one(filter, opts)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(event) = doc_opt {
        if let Some(metadata) = event.metadata {
            if let Some(v) = get_i64(&metadata, "num_pool_validators") {
                return Ok(v as u64);
            }
        }
    }

    Ok(0)
}
