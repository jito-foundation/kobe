// Copied from Stake-o-Matic
use std::{
    collections::HashMap,
    error,
    hash::{Hash, Hasher},
    str::FromStr,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use itertools::Itertools;
use log::*;
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;

use crate::error::KobeCoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Cluster {
    Devnet,
    Testnet,
    MainnetBeta,
}

impl Cluster {
    /// Get cluster value
    pub fn get_cluster(value: &str) -> Result<Cluster, KobeCoreError> {
        match value.to_lowercase().as_ref() {
            "mainnet-beta" | "mainnet" | "m" => Ok(Cluster::MainnetBeta),
            "testnet" | "t" => Ok(Cluster::Testnet),
            "devnet" | "d" => Ok(Cluster::Devnet),
            _ => Err(KobeCoreError::InvalidCluster(value.to_string())),
        }
    }
}

impl std::fmt::Display for Cluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Devnet => "devnet",
                Self::Testnet => "testnet",
                Self::MainnetBeta => "mainnet-beta",
            }
        )
    }
}

#[derive(Debug, Clone, Default)]
pub enum ClusterJson {
    #[default]
    MainnetBeta,
    Testnet,
    Devnet,
    Localhost,
}

impl ClusterJson {
    pub fn from_cluster(cluster: Cluster) -> ClusterJson {
        match cluster {
            Cluster::Devnet => ClusterJson::Devnet,
            Cluster::MainnetBeta => ClusterJson::MainnetBeta,
            Cluster::Testnet => ClusterJson::Testnet,
        }
    }
}

impl AsRef<str> for ClusterJson {
    fn as_ref(&self) -> &str {
        match self {
            Self::MainnetBeta => "mainnet.json",
            Self::Testnet => "testnet.json",
            Self::Devnet => "devnet.json",
            Self::Localhost => "localhost.json",
        }
    }
}

const DEFAULT_BASE_URL: &str = "https://www.validators.app/api/v1/";
const TOKEN_HTTP_HEADER_NAME: &str = "Token";

#[derive(Debug)]
pub struct ClientConfig {
    pub base_url: String,
    pub cluster: ClusterJson,
    pub api_token: String,
    pub timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            cluster: ClusterJson::default(),
            api_token: String::default(),
            timeout: Duration::from_secs(90),
        }
    }
}

#[derive(Debug)]
enum Endpoint {
    Validators,
    CommissionChangeIndex,
}

impl Endpoint {
    fn with_cluster(path: &str, cluster: &ClusterJson) -> String {
        format!("{}/{}", path, cluster.as_ref())
    }
    pub fn path(&self, cluster: &ClusterJson) -> String {
        match self {
            Self::Validators => Self::with_cluster("validators", cluster),
            Self::CommissionChangeIndex => Self::with_cluster("commission-changes", cluster),
        }
    }
}

#[derive(Debug, Deserialize, Default, Serialize, Clone)]
pub struct ValidatorsAppResponseRaw {
    pub account: Option<String>,
    pub active_stake: Option<u64>,
    pub commission: Option<u8>,
    pub consensus_mods_score: Option<i8>,
    pub created_at: Option<String>,
    pub data_center_concentration_score: Option<i64>,
    pub data_center_host: Option<String>,
    pub data_center_key: Option<String>,
    pub delinquent: Option<bool>,
    pub details: Option<String>,
    pub epoch: Option<u64>,
    pub epoch_credits: Option<u64>,
    pub keybase_id: Option<String>,
    pub name: Option<String>,
    pub network: Option<String>,
    pub ping_time: Option<f64>,
    pub published_information_score: Option<i64>,
    pub root_distance_score: Option<i64>,
    pub security_report_score: Option<i64>,
    pub skipped_slot_percent: Option<String>,
    pub skipped_slot_score: Option<i64>,
    pub skipped_slots: Option<u64>,
    pub software_version: Option<String>,
    pub software_version_score: Option<i64>,
    pub stake_concentration_score: Option<i64>,
    pub total_score: Option<i64>,
    pub updated_at: Option<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub vote_account: String,
    pub vote_distance_score: Option<i64>,
    pub www_url: Option<String>,
}

impl PartialEq for ValidatorsAppResponseRaw {
    fn eq(&self, other: &Self) -> bool {
        self.vote_account == other.vote_account && self.epoch == other.epoch
    }
}

impl Eq for ValidatorsAppResponseRaw {}

impl Hash for ValidatorsAppResponseRaw {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.vote_account.hash(state);
        self.epoch.hash(state);
    }
}

#[derive(Debug, Deserialize, Default, Serialize, Clone)]
pub struct ValidatorsAppResponseEntry {
    pub account: Option<String>,
    pub active_stake: Option<u64>,
    pub commission: Option<u8>,
    pub consensus_mods_score: Option<i8>,
    pub created_at: Option<String>,
    pub data_center_concentration_score: Option<i64>,
    pub data_center_host: Option<String>,
    pub data_center_key: Option<String>,
    pub delinquent: Option<bool>,
    pub details: Option<String>,
    pub epoch: Option<u64>,
    pub epoch_credits: Option<u64>,
    pub keybase_id: Option<String>,
    pub name: Option<String>,
    pub network: Option<String>,
    pub ping_time: Option<f64>,
    pub published_information_score: Option<i64>,
    pub root_distance_score: Option<i64>,
    pub security_report_score: Option<i64>,
    pub skipped_slot_percent: Option<String>,
    pub skipped_slot_score: Option<i64>,
    pub skipped_slots: Option<u64>,
    pub software_version: Option<String>,
    pub software_version_score: Option<i64>,
    pub stake_concentration_score: Option<i64>,
    pub total_score: Option<i64>,
    pub updated_at: Option<String>,
    pub url: Option<String>,
    pub vote_account: Pubkey,
    pub vote_distance_score: Option<i64>,
    pub www_url: Option<String>,
}

impl From<ValidatorsAppResponseRaw> for ValidatorsAppResponseEntry {
    fn from(entry: ValidatorsAppResponseRaw) -> Self {
        Self {
            account: entry.account,
            active_stake: entry.active_stake,
            commission: entry.commission,
            consensus_mods_score: entry.consensus_mods_score,
            created_at: entry.created_at,
            data_center_concentration_score: entry.data_center_concentration_score,
            data_center_host: entry.data_center_host,
            data_center_key: entry.data_center_key,
            delinquent: entry.delinquent,
            details: entry.details,
            epoch: entry.epoch,
            epoch_credits: entry.epoch_credits,
            keybase_id: entry.keybase_id,
            name: entry.name,
            network: entry.network,
            ping_time: entry.ping_time,
            published_information_score: entry.published_information_score,
            root_distance_score: entry.root_distance_score,
            security_report_score: entry.security_report_score,
            skipped_slot_percent: entry.skipped_slot_percent,
            skipped_slot_score: entry.skipped_slot_score,
            skipped_slots: entry.skipped_slots,
            software_version: entry.software_version,
            software_version_score: entry.software_version_score,
            stake_concentration_score: entry.stake_concentration_score,
            total_score: entry.total_score,
            updated_at: entry.updated_at,
            url: entry.url,
            vote_account: Pubkey::from_str(&entry.vote_account)
                .expect("Should not use response with malformed vote account"),
            vote_distance_score: entry.vote_distance_score,
            www_url: entry.www_url,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ValidatorsResponse(Vec<ValidatorsAppResponseEntry>);

impl AsRef<Vec<ValidatorsAppResponseEntry>> for ValidatorsResponse {
    fn as_ref(&self) -> &Vec<ValidatorsAppResponseEntry> {
        &self.0
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommissionChangeIndexHistoryEntry {
    pub created_at: String,
    // commission_before can be null; presumably for new validators that have set their commission for the first time
    pub commission_before: Option<f32>,
    pub commission_after: f32,
    pub epoch: u64,
    pub network: String,
    pub id: i32,
    pub epoch_completion: f32,
    pub batch_uuid: String,
    pub account: String,
    // name can be null
    pub name: Option<String>,
}

impl Default for CommissionChangeIndexHistoryEntry {
    fn default() -> CommissionChangeIndexHistoryEntry {
        CommissionChangeIndexHistoryEntry {
            created_at: "".to_string(),
            commission_before: None,
            commission_after: 0.0,
            epoch: 0,
            network: "".to_string(),
            id: 0,
            epoch_completion: 0.0,
            batch_uuid: "".to_string(),
            account: "".to_string(),
            name: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CommissionChangeIndexResponse {
    pub commission_histories: Vec<CommissionChangeIndexHistoryEntry>,
    pub total_count: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum SortKind {
    Score,
    Name,
    Stake,
}

impl std::fmt::Display for SortKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Score => write!(f, "score"),
            Self::Name => write!(f, "name"),
            Self::Stake => write!(f, "stake"),
        }
    }
}

pub type Limit = u32;

#[derive(Clone)]
pub struct Client {
    base_url: reqwest::Url,
    cluster: ClusterJson,
    api_token: String,
    client: reqwest::blocking::Client,
}

pub fn get_validators_app_token_from_env() -> Result<String, String> {
    std::env::var("VALIDATORS_APP_TOKEN").map_err(|err| format!("VALIDATORS_APP_TOKEN: {err}"))
}

impl Client {
    pub fn new<T: AsRef<str>>(api_token: T, cluster: ClusterJson) -> Self {
        let config = ClientConfig {
            api_token: api_token.as_ref().to_string(),
            cluster,
            ..ClientConfig::default()
        };
        Self::new_with_config(config)
    }

    pub fn new_with_cluster(cluster: Cluster) -> Result<Self, Box<dyn error::Error>> {
        let token = get_validators_app_token_from_env()?;
        let client = Self::new(token, ClusterJson::from_cluster(cluster));

        Ok(client)
    }

    pub fn new_with_config(config: ClientConfig) -> Self {
        let ClientConfig {
            base_url,
            cluster,
            api_token,
            timeout,
        } = config;
        Self {
            base_url: reqwest::Url::parse(&base_url).unwrap(),
            cluster,
            api_token,
            client: reqwest::blocking::Client::builder()
                .timeout(timeout)
                .build()
                .unwrap(),
        }
    }

    fn request(
        &self,
        endpoint: Endpoint,
        query: &HashMap<String, String>,
    ) -> reqwest::Result<reqwest::blocking::Response> {
        let url = self.base_url.join(&endpoint.path(&self.cluster)).unwrap();
        let start = Instant::now();
        let request = self
            .client
            .get(url)
            .header(TOKEN_HTTP_HEADER_NAME, &self.api_token)
            .query(&query)
            .build()?;
        let result = self.client.execute(request);
        info!(
            "Validators App response took {:?}",
            Instant::now().duration_since(start)
        );
        result
    }

    pub fn validators(
        &self,
        sort: Option<SortKind>,
        limit: Option<Limit>,
        epoch: u64,
    ) -> reqwest::Result<ValidatorsResponse> {
        let mut query = HashMap::new();
        if let Some(sort) = sort {
            query.insert("sort".into(), sort.to_string());
        }
        if let Some(limit) = limit {
            query.insert("limit".into(), limit.to_string());
        }
        let response = self.request(Endpoint::Validators, &query)?;
        let validators = response.json::<Vec<ValidatorsAppResponseRaw>>()?;
        // Drop any empty strings and non-valid pubkeys, then convert all String values into Pubkeys
        // Also drop non-unique validators
        let filtered_validators: Vec<ValidatorsAppResponseEntry> = validators
            .into_iter()
            .unique()
            .filter(|v| {
                Pubkey::from_str(v.vote_account.as_str()).is_ok()
                    && v.epoch.is_some()
                    && v.epoch.unwrap() == epoch
            })
            .map(|v| v.into())
            .collect();

        Ok(ValidatorsResponse(filtered_validators))
    }

    // See https://www.validators.app/api-documentation#commission-change-index
    // Note that the endpoint returns a different format from what is currently (Jan 2022) documented at this URL, and the endpoint is currently  described as experimental. So this may change.
    pub fn commission_change_index(
        &self,
        date_from: Option<DateTime<Utc>>,
        records_per_page: Option<i32>,
        page: Option<i32>,
    ) -> reqwest::Result<CommissionChangeIndexResponse> {
        let mut query: HashMap<String, String> = HashMap::new();

        if let Some(date_from) = date_from {
            query.insert("date_from".into(), date_from.format("%FT%T").to_string());
        }

        if let Some(records_per_page) = records_per_page {
            query.insert("per".into(), records_per_page.to_string());
        }

        if let Some(page) = page {
            query.insert("page".into(), page.to_string());
        }

        let response = self.request(Endpoint::CommissionChangeIndex, &query)?;
        response.json::<CommissionChangeIndexResponse>()
    }
}
