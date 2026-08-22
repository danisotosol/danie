//! Uniform HTTP error rendering: `{"error": "<message>"}` JSON bodies.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use danie_engine::EngineError;
use danie_llm::LlmError;
use serde_json::json;

/// HTTP-facing wrapper around [`danie_engine::EngineError`] mapping engine
/// failures onto status codes:
///
/// - `Json(_)` and `Cycle` → 422 Unprocessable Entity
/// - `Llm(LlmError::MissingKey { .. })` → 500 (message names the env var)
/// - other `Llm(_)` → 502 Bad Gateway
/// - `Core(_)` → 500 Internal Server Error
#[derive(Debug)]
pub struct EngineApiError(pub EngineError);

impl From<EngineError> for EngineApiError {
    fn from(value: EngineError) -> Self {
        Self(value)
    }
}

impl IntoResponse for EngineApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            EngineError::Json(_) | EngineError::Cycle => StatusCode::UNPROCESSABLE_ENTITY,
            EngineError::Llm(LlmError::MissingKey { .. }) | EngineError::Core(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            EngineError::Llm(_) => StatusCode::BAD_GATEWAY,
        };
        (status, Json(json!({ "error": self.0.to_string() }))).into_response()
    }
}

/// A fully rendered HTTP error response kept behind a box so that `Result`
/// error variants stay small (clippy `result_large_err`).
#[derive(Debug)]
pub struct ErrorResponse(pub std::boxed::Box<Response>);

impl From<EngineApiError> for ErrorResponse {
    fn from(value: EngineApiError) -> Self {
        Self(std::boxed::Box::new(value.into_response()))
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        *self.0
    }
}

/// 400 response for malformed or non-JSON request bodies.
pub fn bad_request(message: impl std::fmt::Display) -> ErrorResponse {
    boxed(StatusCode::BAD_REQUEST, message)
}

/// 422 response for syntactically valid requests carrying unusable values.
pub fn unprocessable(message: impl std::fmt::Display) -> ErrorResponse {
    boxed(StatusCode::UNPROCESSABLE_ENTITY, message)
}

fn boxed(status: StatusCode, message: impl std::fmt::Display) -> ErrorResponse {
    ErrorResponse(Box::new(
        (status, Json(json!({ "error": message.to_string() }))).into_response(),
    ))
}
