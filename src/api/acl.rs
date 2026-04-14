use crate::domain::acl_service::Permission;
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
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

use super::acl_guard::enforce_permission;
use super::error::{ProblemDetails, internal};

/// Get the Access Control List for a document.
#[utoipa::path(
    get,
    path = "/documents/{id}/acl",
    responses(
        (status = 200, description = "ACL rules list", body = [DocumentAcl]),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 404, description = "Not found", body = ProblemDetails)
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
    enforce_permission(&state.pool, &auth, id, Permission::Read).await?;
    let rules = AclRepo::list_by_document_id(&state.pool, id)
        .await
        .map_err(internal)?;
    Ok(Json(rules))
}

/// Replace the Access Control List for a document.
///
/// Only users with admin permission on the document can replace ACL rules.
#[utoipa::path(
    put,
    path = "/documents/{id}/acl",
    request_body = [DocumentAcl],
    responses(
        (status = 204, description = "ACL updated successfully"),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden (requires admin permission)"),
        (status = 404, description = "Not found", body = ProblemDetails)
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
    enforce_permission(&state.pool, &auth, id, Permission::Admin).await?;
    let mut tx = state.pool.begin().await.map_err(internal)?;
    AclRepo::delete_by_document_id(&mut tx, id)
        .await
        .map_err(internal)?;
    for mut r in rules {
        r.id = Uuid::new_v4();
        r.document_id = id;
        AclRepo::create(&mut tx, &r).await.map_err(internal)?;
    }
    let audit = AuditLog::builder(auth.user_id, "document.acl.replace", "document", id).build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// A single ACL patch operation (add or remove an entry).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AclPatchEntry {
    /// Operation: "add" or "remove".
    pub op: String,
    /// Type of principal: "user" or "role".
    pub principal_type: String,
    /// User ID (required when principal_type is "user").
    pub principal_id: Option<Uuid>,
    /// Role name (required when principal_type is "role").
    pub role: Option<String>,
    /// Permission to grant or revoke: "read", "write", or "admin".
    pub permission: String,
}

/// Granular ACL modifications for a document.
///
/// Allows adding or removing individual ACL entries without replacing the entire list.
/// Only users with admin permission on the document can patch ACL rules.
#[utoipa::path(
    patch,
    path = "/documents/{id}/acl",
    request_body = [AclPatchEntry],
    responses(
        (status = 204, description = "ACL patched successfully"),
        (status = 400, description = "Bad request", body = ProblemDetails),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden (requires admin permission)"),
        (status = 404, description = "Not found", body = ProblemDetails)
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["document:write"])),
    tag = "ACL"
)]
pub async fn patch_acl(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
    Json(ops): Json<Vec<AclPatchEntry>>,
) -> Result<StatusCode, ProblemDetails> {
    auth.require_scope("document:write")?;
    enforce_permission(&state.pool, &auth, id, Permission::Admin).await?;

    let mut tx = state.pool.begin().await.map_err(internal)?;

    for entry in ops {
        match entry.op.as_str() {
            "add" => {
                let acl = DocumentAcl {
                    id: Uuid::new_v4(),
                    document_id: id,
                    principal_type: entry.principal_type,
                    principal_id: entry.principal_id,
                    role: entry.role,
                    permission: entry.permission,
                };
                AclRepo::create(&mut tx, &acl).await.map_err(internal)?;
            }
            "remove" => {
                sqlx::query(
                    "DELETE FROM document_acl WHERE document_id = $1 \
                     AND principal_type = $2 \
                     AND (principal_id = $3 OR ($3 IS NULL AND principal_id IS NULL)) \
                     AND (role = $4 OR ($4 IS NULL AND role IS NULL)) \
                     AND permission = $5",
                )
                .bind(id)
                .bind(&entry.principal_type)
                .bind(entry.principal_id)
                .bind(&entry.role)
                .bind(&entry.permission)
                .execute(&mut *tx)
                .await
                .map_err(internal)?;
            }
            _ => {
                return Err(super::error::bad_request(format!(
                    "unknown op: '{}', expected 'add' or 'remove'",
                    entry.op
                )));
            }
        }
    }

    let audit = AuditLog::builder(auth.user_id, "document.acl.patch", "document", id).build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
