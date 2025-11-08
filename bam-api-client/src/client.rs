use std::time::Duration;

use reqwest::{Client, Method, Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};

use crate::{config::Config, error::BamApiError, types::ValidatorsResponse};

/// Main client for interacting with BAM APIs
#[derive(Debug, Clone)]
pub struct BamApiClient {
    /// Reqwest client
    client: Client,

    /// Client config
    config: Config,
}

impl BamApiClient {
    /// Create a new Jito API client with the given configuration
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to build HTTP client");

        Self { client, config }
    }

    /// Make a GET request
    async fn get<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &str,
    ) -> Result<T, BamApiError> {
        let url = format!(
            "{}/api/{}{}{}",
            self.config.base_url,
            crate::API_VERSION,
            endpoint,
            query
        );
        self.request(Method::GET, &url, None::<&()>).await
    }

    /// Make an HTTP request with optional retry logic
    async fn request<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        url: &str,
        body: Option<&B>,
    ) -> Result<T, BamApiError> {
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
    ) -> Result<T, BamApiError> {
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
                StatusCode::NOT_FOUND => Err(BamApiError::NotFound(error_text)),
                StatusCode::TOO_MANY_REQUESTS => Err(BamApiError::RateLimitExceeded),
                StatusCode::REQUEST_TIMEOUT => Err(BamApiError::Timeout),
                _ => Err(BamApiError::api_error(status_code, error_text)),
            }
        }
    }

    /// Determine if an error should trigger a retry
    fn should_retry(&self, error: &BamApiError) -> bool {
        matches!(error, BamApiError::Timeout | BamApiError::HttpError(_))
    }

    /// Get all validators
    ///
    /// Returns validator state.
    pub async fn get_validators(&self) -> Result<Vec<ValidatorsResponse>, BamApiError> {
        self.get("/validators", "").await
    }
}
