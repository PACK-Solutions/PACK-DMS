use axum::{
    Router,
    response::Html,
    routing::get,
};

/// Serves the single-page administration frontend.
///
/// The UI is built with HTMX for dynamic interactions and Tailwind CSS
/// for styling, both loaded from CDN. No build step required.
async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

pub fn router() -> Router {
    Router::new().route("/admin", get(index))
}
