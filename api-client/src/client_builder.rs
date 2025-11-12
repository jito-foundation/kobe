use std::time::Duration;

use crate::{client::KobeApiClient, config::Config};

/// Builder for creating a KobeApiClient with custom configuration
pub struct KobeApiClientBuilder {
    config: Config,
}

impl KobeApiClientBuilder {
    /// Create a new builder with mainnet defaults
    pub fn new() -> Self {
        Self {
            config: Config::mainnet(),
        }
    }

    /// Set the base URL
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.config.base_url = base_url.into();
        self
    }

    /// Set the request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Set the user agent
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.config.user_agent = user_agent.into();
        self
    }

    /// Enable or disable retries
    pub fn retry(mut self, enabled: bool) -> Self {
        self.config.retry_enabled = enabled;
        self
    }

    /// Set maximum number of retries
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    /// Build the client
    pub fn build(self) -> KobeApiClient {
        KobeApiClient::new(self.config)
    }
}

impl Default for KobeApiClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
