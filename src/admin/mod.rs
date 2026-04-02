use axum::{
    Router,
    response::{Html, IntoResponse},
    routing::get,
};
use axum::http::header;

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
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("js/utils.js"))
}

async fn js_app() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("js/app.js"))
}

async fn js_documents() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("js/documents.js"))
}

async fn js_versions() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("js/versions.js"))
}

async fn js_preview() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("js/preview.js"))
}

async fn js_acl() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("js/acl.js"))
}

async fn js_audit() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], include_str!("js/audit.js"))
}

pub fn router() -> Router {
    Router::new()
        .route("/admin", get(index))
        .route("/admin/js/utils.js", get(js_utils))
        .route("/admin/js/app.js", get(js_app))
        .route("/admin/js/documents.js", get(js_documents))
        .route("/admin/js/versions.js", get(js_versions))
        .route("/admin/js/preview.js", get(js_preview))
        .route("/admin/js/acl.js", get(js_acl))
        .route("/admin/js/audit.js", get(js_audit))
}
