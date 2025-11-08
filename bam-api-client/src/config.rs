use std::time::Duration;

/// Configuration for the Jito API client
#[derive(Debug, Clone)]
pub struct Config {
    /// Base URL for the API
    pub base_url: String,

    /// Request timeout in seconds
    pub timeout: Duration,

    /// Enable retry on failure
    pub retry_enabled: bool,

    /// Maximum number of retries
    pub max_retries: u32,
}

impl Config {
    /// Create a custom configuration
    pub fn custom(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout: Duration::from_secs(30),
            retry_enabled: true,
            max_retries: 3,
        }
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable or disable retries
    pub fn with_retry(mut self, enabled: bool) -> Self {
        self.retry_enabled = enabled;
        self
    }

    /// Set maximum number of retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }
}
