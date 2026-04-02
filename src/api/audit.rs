use crate::domain::models::AuditLog;
use crate::infra::{
    auth::{AppState, JwtAuth},
    db::AuditRepo,
};
use axum::{
    Json,
    extract::{Query, State},
};
use std::sync::Arc;

use super::error::{ProblemDetails, forbidden, internal};
use super::types::SearchQuery;

/// List system-wide audit logs.
///
/// Provides a paginated history of all actions performed in the system.
#[utoipa::path(
    get,
    path = "/audit",
    responses(
        (status = 200, description = "List of audit logs", body = [AuditLog]),
        (status = 401, description = "Unauthorized")
    ),
    params(
        ("limit" = Option<i64>, Query, description = "Page size"),
        ("offset" = Option<i64>, Query, description = "Page offset")
    ),
    security(("bearerAuth"=["admin"])),
    tag = "Audit"
)]
pub async fn list_audit(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Query(p): Query<SearchQuery>,
) -> Result<Json<Vec<AuditLog>>, ProblemDetails> {
    if !auth.has_role("admin") {
        return Err(forbidden("Missing admin role"));
    }
    let logs = AuditRepo::list_all(&state.pool, p.limit.unwrap_or(100), p.offset.unwrap_or(0))
        .await
        .map_err(internal)?;
    Ok(Json(logs))
}
