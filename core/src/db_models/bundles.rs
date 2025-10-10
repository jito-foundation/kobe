use chrono::{serde::ts_seconds, DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Bundles {
    pub id: String,
    pub start_tx: u32,
    pub end_tx: u32,
    pub slot_id: u64,
    #[serde(with = "ts_seconds")]
    pub block_time: DateTime<Utc>,
    pub tip_amount: u64,
    pub executor: String,
}
