use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

/// RFC 9457 Problem Details for HTTP APIs.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProblemDetails {
    /// A URI reference that identifies the problem type.
    #[serde(rename = "type")]
    #[schema(example = "about:blank")]
    pub problem_type: String,

    /// A short, human-readable summary of the problem.
    #[schema(example = "Not Found")]
    pub title: String,

    /// The HTTP status code.
    #[schema(example = 404)]
    pub status: u16,

    /// A human-readable explanation specific to this occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(example = "Document with the given ID was not found")]
    pub detail: Option<String>,

    /// A URI reference that identifies the specific occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl ProblemDetails {
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self {
            problem_type: "about:blank".to_string(),
            title: canonical_reason(status).to_string(),
            status: status.as_u16(),
            detail: Some(detail.into()),
            instance: None,
        }
    }
}

impl From<(StatusCode, String)> for ProblemDetails {
    fn from((status, detail): (StatusCode, String)) -> Self {
        Self::new(status, detail)
    }
}

impl IntoResponse for ProblemDetails {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        if status.is_client_error() {
            tracing::warn!(
                status = self.status,
                title = %self.title,
                detail = ?self.detail,
                "Client error response"
            );
        } else if status.is_server_error() {
            tracing::error!(
                status = self.status,
                title = %self.title,
                detail = ?self.detail,
                "Server error response"
            );
        }
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            "application/problem+json"
                .parse()
                .expect("valid header value"),
        );
        response
    }
}

fn canonical_reason(status: StatusCode) -> &'static str {
    status.canonical_reason().unwrap_or("Unknown Error")
}

/// Converts any `Debug`-able error into a 500 ProblemDetails response.
///
/// The original error is logged server-side but **not** exposed to the client.
pub fn internal<E: std::fmt::Debug>(e: E) -> ProblemDetails {
    tracing::error!(error = ?e, "internal server error");
    ProblemDetails::new(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
}

/// Helper to create a not-found ProblemDetails.
pub fn not_found(detail: &str) -> ProblemDetails {
    ProblemDetails::new(StatusCode::NOT_FOUND, detail)
}

/// Helper to create a bad-request ProblemDetails.
pub fn bad_request(detail: impl Into<String>) -> ProblemDetails {
    ProblemDetails::new(StatusCode::BAD_REQUEST, detail)
}

/// Helper to create a forbidden ProblemDetails.
pub fn forbidden(detail: impl Into<String>) -> ProblemDetails {
    ProblemDetails::new(StatusCode::FORBIDDEN, detail)
}

/// Helper to create an unauthorized ProblemDetails.
pub fn unauthorized(detail: impl Into<String>) -> ProblemDetails {
    ProblemDetails::new(StatusCode::UNAUTHORIZED, detail)
}
