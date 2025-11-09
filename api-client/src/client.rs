use std::time::Duration;

use kobe_api::schemas::{
    jitosol_ratio::JitoSolRatioResponse,
    mev_rewards::{MevRewards, StakerRewardsResponse, ValidatorRewardsResponse},
    stake_pool_stats::{DateTimeRangeFilter, GetStakePoolStatsRequest, GetStakePoolStatsResponse},
    validator::{
        AverageMevCommissionOverTimeResponse, JitoStakeOverTimeResponse,
        ValidatorByVoteAccountResponse, ValidatorsResponse,
    },
};
use reqwest::{Client, Method, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    config::Config,
    error::KobeApiError,
    request_type::{EpochRequest, QueryParams},
    response_type::DailyMevRewards,
};

/// Main client for interacting with Jito APIs
#[derive(Debug, Clone)]
pub struct KobeApiClient {
    /// Reqwest client
    client: Client,
    config: Config,
}

impl KobeApiClient {
    /// Create a new Jito API client with the given configuration
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .build()
            .expect("Failed to build HTTP client");

        Self { client, config }
    }

    /// Create a client with mainnet defaults
    pub fn mainnet() -> Self {
        Self::new(Config::mainnet())
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Make a GET request
    async fn get<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &str,
    ) -> Result<T, KobeApiError> {
        let url = format!(
            "{}/api/{}{}{}",
            self.config.base_url,
            crate::API_VERSION,
            endpoint,
            query
        );
        self.request(Method::GET, &url, None::<&()>).await
    }

    /// Make a POST request
    async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: Option<&B>,
    ) -> Result<T, KobeApiError> {
        let url = format!(
            "{}/api/{}{}",
            self.config.base_url,
            crate::API_VERSION,
            endpoint
        );
        self.request(Method::POST, &url, body).await
    }

    /// Make an HTTP request with optional retry logic
    async fn request<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        url: &str,
        body: Option<&B>,
    ) -> Result<T, KobeApiError> {
        let mut retries = 0;
        let max_retries = if self.config.retry_enabled {
            self.config.max_retries
        } else {
            0
        };

        loop {
            let mut request = self.client.request(method.clone(), url);

            if let Some(body) = body {
                request = request.json(body);
            }

            let response = request.send().await?;

            match self.handle_response(response).await {
                Ok(data) => return Ok(data),
                Err(e) => {
                    if retries >= max_retries || !self.should_retry(&e) {
                        return Err(e);
                    }
                    retries += 1;
                    // Exponential backoff
                    let delay = Duration::from_millis(100 * 2u64.pow(retries));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Handle HTTP response
    async fn handle_response<T: DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T, KobeApiError> {
        let status = response.status();

        if status.is_success() {
            response.json::<T>().await.map_err(Into::into)
        } else {
            let status_code = status.as_u16();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            match status {
                StatusCode::NOT_FOUND => Err(KobeApiError::NotFound(error_text)),
                StatusCode::TOO_MANY_REQUESTS => Err(KobeApiError::RateLimitExceeded),
                StatusCode::REQUEST_TIMEOUT => Err(KobeApiError::Timeout),
                _ => Err(KobeApiError::api_error(status_code, error_text)),
            }
        }
    }

    /// Determine if an error should trigger a retry
    fn should_retry(&self, error: &KobeApiError) -> bool {
        matches!(error, KobeApiError::Timeout | KobeApiError::HttpError(_))
    }

    /// Get staker rewards
    ///
    /// Retrieves individual claimable MEV and priority fee rewards from the tip distribution merkle trees.
    ///
    /// # Arguments
    ///
    /// * `limit` - Optional limit on the number of results (default: API default)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use kobe_api_client::client::KobeApiClient;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = KobeApiClient::mainnet();
    /// let rewards = client.get_staker_rewards(Some(10)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_staker_rewards(
        &self,
        limit: Option<u32>,
    ) -> Result<StakerRewardsResponse, KobeApiError> {
        let query = if let Some(limit) = limit {
            format!("?limit={}", limit)
        } else {
            String::new()
        };
        self.get("/staker_rewards", &query).await
    }

    /// Get staker rewards with full query parameters
    pub async fn get_staker_rewards_with_params(
        &self,
        params: &QueryParams,
    ) -> Result<StakerRewardsResponse, KobeApiError> {
        self.get("/staker_rewards", &params.to_query_string()).await
    }

    /// Get validator rewards for a specific epoch
    ///
    /// Retrieves aggregated MEV and priority fee rewards data per validator.
    ///
    /// # Arguments
    ///
    /// * `epoch` - Epoch number (optional, defaults to latest)
    /// * `limit` - Optional limit on the number of results
    pub async fn get_validator_rewards(
        &self,
        epoch: Option<u64>,
        limit: Option<u32>,
    ) -> Result<ValidatorRewardsResponse, KobeApiError> {
        let mut params = Vec::new();

        if let Some(epoch) = epoch {
            params.push(format!("epoch={}", epoch));
        }
        if let Some(limit) = limit {
            params.push(format!("limit={}", limit));
        }

        let query = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };

        self.get("/validator_rewards", &query).await
    }

    /// Get all validators for a given epoch
    ///
    /// Returns validator state for a given epoch (defaults to latest).
    ///
    /// # Arguments
    ///
    /// * `epoch` - Optional epoch number (defaults to latest)
    pub async fn get_validators(
        &self,
        epoch: Option<u64>,
    ) -> Result<ValidatorsResponse, KobeApiError> {
        if let Some(epoch) = epoch {
            self.post("/validators", Some(&EpochRequest { epoch }))
                .await
        } else {
            self.post::<EpochRequest, _>("/validators", None).await
        }
    }

    /// Get JitoSOL stake pool validators for a given epoch
    ///
    /// Returns only validators that are actively part of the JitoSOL validator set.
    pub async fn get_jitosol_validators(
        &self,
        epoch: Option<u64>,
    ) -> Result<ValidatorsResponse, KobeApiError> {
        if let Some(epoch) = epoch {
            self.post("/jitosol_validators", Some(&EpochRequest { epoch }))
                .await
        } else {
            self.post::<EpochRequest, _>("/jitosol_validators", None)
                .await
        }
    }

    /// Get historical data for a single validator
    ///
    /// Returns historical reward data for a validator, sorted by epoch (descending).
    ///
    /// # Arguments
    ///
    /// * `vote_account` - The validator's vote account public key
    pub async fn get_validator_info_by_vote_account(
        &self,
        vote_account: &str,
    ) -> Result<Vec<ValidatorByVoteAccountResponse>, KobeApiError> {
        self.get(&format!("/validators/{}", vote_account), "").await
    }

    /// Get MEV rewards network statistics for an epoch
    ///
    /// Returns network-level statistics including total MEV, stake weight, and reward per lamport.
    ///
    /// # Arguments
    ///
    /// * `epoch` - Optional epoch number (defaults to latest)
    pub async fn get_mev_rewards(&self, epoch: Option<u64>) -> Result<MevRewards, KobeApiError> {
        if let Some(epoch) = epoch {
            self.post("/mev_rewards", Some(&EpochRequest { epoch }))
                .await
        } else {
            // GET request for latest epoch
            self.get("/mev_rewards", "").await
        }
    }

    /// Get daily MEV rewards
    ///
    /// Returns aggregated MEV rewards per calendar day.
    pub async fn get_daily_mev_rewards(&self) -> Result<Vec<DailyMevRewards>, KobeApiError> {
        self.get("/daily_mev_rewards", "").await
    }

    /// Get Jito stake over time
    ///
    /// Returns a map of epoch to percentage of all Solana stake delegated to Jito-running validators.
    pub async fn get_jito_stake_over_time(
        &self,
    ) -> Result<JitoStakeOverTimeResponse, KobeApiError> {
        self.get("/jito_stake_over_time", "").await
    }

    /// Get MEV commission average over time
    ///
    /// Returns stake-weighted average MEV commission along with other metrics.
    pub async fn get_mev_commission_average_over_time(
        &self,
    ) -> Result<AverageMevCommissionOverTimeResponse, KobeApiError> {
        self.get("/mev_commission_average_over_time", "").await
    }

    /// Get JitoSOL to SOL exchange ratio over time
    ///
    /// # Arguments
    ///
    /// * `start` - Start datetime for the range
    /// * `end` - End datetime for the range
    pub async fn get_jitosol_sol_ratio(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<JitoSolRatioResponse, KobeApiError> {
        let request = DateTimeRangeFilter { start, end };
        self.post("/jitosol_sol_ratio", Some(&request)).await
    }

    /// Get stake pool statistics
    ///
    /// Returns stake pool analytics including TVL, APY, validator count, supply metrics,
    /// and aggregated MEV rewards over time.
    pub async fn get_stake_pool_stats(
        &self,
        request: Option<&GetStakePoolStatsRequest>,
    ) -> Result<GetStakePoolStatsResponse, KobeApiError> {
        if let Some(req) = request {
            self.post("/stake_pool_stats", Some(req)).await
        } else {
            // GET request for default (last 7 days)
            self.get("/stake_pool_stats", "").await
        }
    }

    /// Get the current epoch from the latest MEV rewards data
    pub async fn get_current_epoch(&self) -> Result<u64, KobeApiError> {
        let mev_rewards = self.get_mev_rewards(None).await?;
        Ok(mev_rewards.epoch)
    }

    /// Calculate total MEV rewards for a time period
    pub async fn calculate_total_mev_rewards(
        &self,
        start_epoch: u64,
        end_epoch: u64,
    ) -> Result<u64, KobeApiError> {
        let mut total = 0u64;

        for epoch in start_epoch..=end_epoch {
            if let Ok(mev_rewards) = self.get_mev_rewards(Some(epoch)).await {
                total = total.saturating_add(mev_rewards.total_network_mev_lamports);
            }
        }

        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{client_builder::KobeApiClientBuilder, request_type::QueryParams};

    use super::Config;

    #[test]
    fn test_config_builder() {
        let config = Config::mainnet()
            .with_timeout(Duration::from_secs(60))
            .with_user_agent("test-agent")
            .with_retry(false);

        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.user_agent, "test-agent");
        assert!(!config.retry_enabled);
    }

    #[test]
    fn test_query_params() {
        let params = QueryParams::default().limit(10).offset(20).epoch(600);

        let query = params.to_query_string();
        assert!(query.contains("limit=10"));
        assert!(query.contains("offset=20"));
        assert!(query.contains("epoch=600"));
    }

    #[test]
    fn test_client_builder() {
        let client = KobeApiClientBuilder::new()
            .timeout(Duration::from_secs(45))
            .retry(true)
            .max_retries(5)
            .build();

        assert_eq!(client.config.timeout, Duration::from_secs(45));
        assert!(client.config.retry_enabled);
        assert_eq!(client.config.max_retries, 5);
    }
}
