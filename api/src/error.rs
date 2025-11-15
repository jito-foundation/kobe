use std::convert::Infallible;

use axum::{response::IntoResponse, BoxError, Json};
use http::StatusCode;
use serde_json::json;
use thiserror::Error;

use crate::resolvers::error::QueryResolverError;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Request validation failed: {message}")]
    ValidationError { message: String },

    #[error("Invalid request: {message}")]
    BadRequest { message: String },

    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("External service error: {service}")]
    ExternalService { service: String },

    #[error("Database operation failed")]
    Database,

    #[error("Internal server error")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            ApiError::ValidationError { message } => (
                StatusCode::BAD_REQUEST,
                format!("Request validation failed: {message}"),
            ),
            ApiError::BadRequest { message } => (StatusCode::BAD_REQUEST, message.clone()),
            ApiError::NotFound { resource } => {
                (StatusCode::NOT_FOUND, format!("{resource} not found"))
            }
            ApiError::ExternalService { service } => (
                StatusCode::BAD_GATEWAY,
                format!("{service} is temporarily unavailable"),
            ),
            ApiError::Database => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database operation failed".to_string(),
            ),
            ApiError::Internal(_msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An internal error occurred".to_string(),
            ),
        };

        let error_response = json!({
            "error": {
                "code": status.as_u16(),
                "message": message,
            }
        });

        (status, Json(error_response)).into_response()
    }
}

// Convert QueryResolverError to ApiError
impl From<QueryResolverError> for ApiError {
    fn from(err: QueryResolverError) -> Self {
        match err {
            QueryResolverError::InvalidRequest(msg) => ApiError::BadRequest { message: msg },
            QueryResolverError::DataStoreError(_) | QueryResolverError::MongoDBError(_) => {
                ApiError::Database
            }
            QueryResolverError::ReqwestError(_) => ApiError::ExternalService {
                service: "External API".to_string(),
            },
            QueryResolverError::RpcError(_) => ApiError::ExternalService {
                service: "Solana RPC".to_string(),
            },
            QueryResolverError::ValidatorHistoryError(msg) => ApiError::Internal(msg),
            QueryResolverError::CustomError(msg) => ApiError::Internal(msg),
            QueryResolverError::JitoTransactionError(err) => ApiError::Internal(err.to_string()),
        }
    }
}

impl ApiError {
    /// Initiallize BadRequest Error
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
        }
    }

    /// Initiallize Validation Error
    pub fn validation_error(message: impl Into<String>) -> Self {
        Self::ValidationError {
            message: message.into(),
        }
    }

    /// Initiallize NotFound Error
    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::NotFound {
            resource: resource.into(),
        }
    }
}

pub async fn handle_error(error: BoxError) -> Result<impl IntoResponse, Infallible> {
    if error.is::<tower::timeout::error::Elapsed>() {
        return Ok((
            StatusCode::REQUEST_TIMEOUT,
            Json(json!({
                "code" : 408,
                "error" : "Request Timeout",
            })),
        ));
    };
    if error.is::<tower::load_shed::error::Overloaded>() {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "code" : 503,
                "error" : "Service Unavailable",
            })),
        ));
    }

    Ok((
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "code" : 500,
            "error" : "Internal Server Error",
        })),
    ))
}
