//! Unified API error type for all handlers.
//!
//! Provides consistent JSON error responses with appropriate HTTP status codes.
//! Internal errors are logged but not exposed to the client.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};

/// Structured API error that maps to appropriate HTTP status codes.
///
/// The `Internal` variant logs the real error via tracing and returns a
/// generic message to the caller — never expose stack traces or internal
/// details to external clients.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::Internal(msg) => {
                tracing::error!(error = %msg, "internal API error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_owned(),
                )
            }
        };

        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}
