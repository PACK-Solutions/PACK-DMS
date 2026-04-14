use axum::http::header;
use axum::{
    Json, Router,
    extract::Query,
    response::{Html, IntoResponse},
    routing::get,
};
use serde::{Deserialize, Serialize};

/// Serves the single-page administration frontend.
///
/// The UI is built with HTMX for dynamic interactions and Tailwind CSS
/// for styling, both loaded from CDN. No build step required.
/// JavaScript is split into focused modules for maintainability.
async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

/// Serves a JavaScript file with the correct `application/javascript` content type.
async fn js_utils() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("js/utils.js"),
    )
}

async fn js_app() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("js/app.js"),
    )
}

async fn js_documents() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("js/documents.js"),
    )
}

async fn js_versions() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("js/versions.js"),
    )
}

async fn js_preview() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("js/preview.js"),
    )
}

async fn js_acl() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("js/acl.js"),
    )
}

async fn js_audit() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("js/audit.js"),
    )
}

#[derive(Deserialize)]
struct GenerateTokenParams {
    /// `"user"` or `"admin"` (default: `"user"`)
    #[serde(default = "default_profile")]
    profile: String,
}

fn default_profile() -> String {
    "user".to_string()
}

#[derive(Serialize)]
struct GenerateTokenResponse {
    token: String,
    profile: String,
    email: String,
    scopes: String,
}

/// Generate a dev JWT token for quick testing.
///
/// This reads the private key from `data/keys/private.pem` and issues a
/// long-lived token. **Only intended for development use.**
async fn generate_token(
    Query(params): Query<GenerateTokenParams>,
) -> Result<Json<GenerateTokenResponse>, (axum::http::StatusCode, String)> {
    let private_pem = std::fs::read_to_string("data/keys/private.pem").map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot read private key: {e}. Run `cargo run --example gen_jwks` first."),
        )
    })?;

    let issuer =
        std::env::var("JWT_ISSUER").unwrap_or_else(|_| "https://example.com/auth".to_string());
    let kid = "default-kid";
    let ttl_secs = 10 * 365 * 24 * 3600; // ~10 years

    let (user_id, email, scopes) = match params.profile.as_str() {
        "admin" => (
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            "admin@example.com",
            "document:read document:write admin",
        ),
        _ => (
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            "user@example.com",
            "document:read document:write",
        ),
    };

    let token = crate::infra::auth::issue_test_jwt(
        &private_pem,
        kid,
        &issuer,
        user_id,
        email,
        scopes,
        ttl_secs,
    )
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Token generation failed: {e}"),
        )
    })?;

    Ok(Json(GenerateTokenResponse {
        token,
        profile: params.profile,
        email: email.to_string(),
        scopes: scopes.to_string(),
    }))
}

pub fn router() -> Router {
    Router::new()
        .route("/admin", get(index))
        .route("/admin/api/generate-token", get(generate_token))
        .route("/admin/js/utils.js", get(js_utils))
        .route("/admin/js/app.js", get(js_app))
        .route("/admin/js/documents.js", get(js_documents))
        .route("/admin/js/versions.js", get(js_versions))
        .route("/admin/js/preview.js", get(js_preview))
        .route("/admin/js/acl.js", get(js_acl))
        .route("/admin/js/audit.js", get(js_audit))
}
