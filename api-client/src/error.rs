use thiserror::Error;

/// Error types for Jito API client
#[derive(Error, Debug)]
pub enum KobeApiError {
    /// HTTP request error
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// API returned an error response
    #[error("API error: {status_code} - {message}")]
    ApiError { status_code: u16, message: String },

    /// Invalid parameter provided
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    /// Resource not found
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded. Please try again later.")]
    RateLimitExceeded,

    /// Timeout error
    #[error("Request timed out")]
    Timeout,

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    /// Other errors
    #[error("An error occurred: {0}")]
    Other(String),
}

impl KobeApiError {
    /// Create an API error from status code and message
    pub fn api_error(status_code: u16, message: impl Into<String>) -> Self {
        KobeApiError::ApiError {
            status_code,
            message: message.into(),
        }
    }

    /// Create an invalid parameter error
    pub fn invalid_parameter(message: impl Into<String>) -> Self {
        KobeApiError::InvalidParameter(message.into())
    }

    /// Check if error is a rate limit error
    pub fn is_rate_limit(&self) -> bool {
        matches!(self, KobeApiError::RateLimitExceeded)
    }

    /// Check if error is a not found error
    pub fn is_not_found(&self) -> bool {
        matches!(self, KobeApiError::NotFound(_))
    }
}
