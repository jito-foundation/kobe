use kobe_core::db_models::steward_events::StewardEvent as StewardEventModel;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StewardEvent {
    pub signature: String,
    pub event_type: String,
    pub vote_account: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub data: serde_json::Value,
    pub epoch: u64,
    pub tx_error: Option<String>,
}

impl From<StewardEventModel> for StewardEvent {
    fn from(event: StewardEventModel) -> Self {
        StewardEvent {
            signature: event.signature,
            epoch: event.epoch,
            event_type: event.event_type,
            timestamp: event
                .timestamp
                .unwrap_or(mongodb::bson::DateTime::from_millis(0))
                .to_chrono(),
            data: event
                .metadata
                .map(|m| serde_json::to_value(&m).unwrap_or_default())
                .unwrap_or_default(),
            vote_account: event.vote_account.unwrap_or_default(),
            tx_error: event.tx_error,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StewardEventsResponse {
    pub events: Vec<StewardEvent>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StewardEventsRequest {
    pub event_type: Option<String>,
    pub vote_account: Option<String>,
    pub epoch: Option<u64>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

impl std::fmt::Display for StewardEventsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "StewardEventsRequest {{ event_type: {:?}, vote_account: {:?}, epoch: {:?}, page: {:?}, limit: {:?} }}",
            self.event_type, self.vote_account, self.epoch, self.page, self.limit
        )
    }
}
