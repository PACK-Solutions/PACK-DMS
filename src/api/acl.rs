use crate::domain::models::*;
use crate::infra::{
    auth::{AppState, JwtAuth},
    db::{AclRepo, AuditRepo},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::sync::Arc;
use uuid::Uuid;

use super::error::{ProblemDetails, forbidden, internal};

/// Get the Access Control List for a document.
#[utoipa::path(
    get,
    path = "/documents/{id}/acl",
    responses(
        (status = 200, description = "ACL rules list", body = [DocumentAcl]),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Document not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["document:read"])),
    tag = "ACL"
)]
pub async fn get_acl(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DocumentAcl>>, ProblemDetails> {
    auth.require_scope("document:read")?;
    let rules = AclRepo::list_by_document_id(&state.pool, id)
        .await
        .map_err(internal)?;
    Ok(Json(rules))
}

/// Replace the Access Control List for a document.
///
/// Only administrators can update ACL rules.
#[utoipa::path(
    put,
    path = "/documents/{id}/acl",
    request_body = [DocumentAcl],
    responses(
        (status = 204, description = "ACL updated successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (requires admin role)"),
        (status = 404, description = "Document not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["document:write"])),
    tag = "ACL"
)]
pub async fn put_acl(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
    Json(rules): Json<Vec<DocumentAcl>>,
) -> Result<StatusCode, ProblemDetails> {
    auth.require_scope("document:write")?;
    if !auth.has_role("admin") {
        return Err(forbidden("forbidden"));
    }
    let mut tx = state.pool.begin().await.map_err(internal)?;
    AclRepo::delete_by_document_id(&mut tx, id)
        .await
        .map_err(internal)?;
    for mut r in rules {
        r.id = Uuid::new_v4();
        r.document_id = id;
        AclRepo::create(&mut tx, &r).await.map_err(internal)?;
    }
    let audit = AuditLog::builder(auth.user_id, "document.acl", "document", id).build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
