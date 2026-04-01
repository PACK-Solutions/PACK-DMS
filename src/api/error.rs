use axum::http::StatusCode;

/// Converts any `Debug`-able error into a 500 Internal Server Error response.
pub fn internal<E: std::fmt::Debug>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:?}"))
}
