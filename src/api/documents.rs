use crate::domain::models::*;
use crate::domain::acl_service::{AclService, Permission};
use crate::infra::{
    auth::{AppState, JwtAuth},
    db::{AclRepo, AuditRepo, DocumentRepo},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use super::acl_guard::enforce_permission;
use super::error::{ProblemDetails, bad_request, forbidden, internal, not_found};
use super::types::{
    CreateDocumentRequest, DocumentResponse, LegalHoldRequest, PatchDocumentRequest,
    RetentionRequest, SearchQuery, StatusChangeRequest, MAX_TITLE_LENGTH,
};

/// Validate that a document title is non-empty and within the allowed length.
fn validate_title(title: &str) -> Result<(), ProblemDetails> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(bad_request("title must not be empty"));
    }
    if trimmed.len() > MAX_TITLE_LENGTH {
        return Err(bad_request(format!(
            "title must not exceed {MAX_TITLE_LENGTH} characters"
        )));
    }
    Ok(())
}

/// Create a new document shell.
///
/// Documents are created in 'draft' status with no initial content.
/// Use the versions endpoint to upload the actual document file.
#[utoipa::path(
    post,
    path = "/documents",
    request_body = CreateDocumentRequest,
    responses(
        (status = 201, description = "Document created", body = DocumentResponse),
        (status = 400, description = "Bad request", body = ProblemDetails),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden", body = ProblemDetails)
    ),
    security(("bearerAuth" = ["document:write"])),
    tag = "Documents"
)]
pub async fn create_document(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Json(req): Json<CreateDocumentRequest>,
) -> Result<(StatusCode, Json<DocumentResponse>), ProblemDetails> {
    auth.require_scope("document:write")?;
    validate_title(&req.title)?;
    let mut tx = state.pool.begin().await.map_err(internal)?;
    let now = Utc::now();
    let doc = Document {
        id: Uuid::new_v4(),
        title: req.title,
        status: doc_status::DRAFT.to_string(),
        owner_id: auth.user_id,
        current_version_id: None,
        legal_hold: false,
        retention_until: None,
        metadata: req.metadata,
        created_at: now,
        updated_at: now,
        deleted_at: None,
        deleted_by: None,
        archived_at: None,
        parent_id: None,
    };
    DocumentRepo::create(&mut tx, &doc)
        .await
        .map_err(internal)?;

    // Auto-create a default ACL entry granting admin permission to the document owner.
    let owner_acl = DocumentAcl {
        id: Uuid::new_v4(),
        document_id: doc.id,
        principal_type: "user".to_string(),
        principal_id: Some(auth.user_id),
        role: None,
        permission: "admin".to_string(),
    };
    AclRepo::create(&mut tx, &owner_acl)
        .await
        .map_err(internal)?;

    let audit = AuditLog::builder(auth.user_id, "document.create", "document", doc.id).build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok((StatusCode::CREATED, Json(doc.into())))
}

/// Get document metadata by ID.
#[utoipa::path(
    get,
    path = "/documents/{id}",
    responses(
        (status = 200, description = "Document found", body = DocumentResponse),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 404, description = "Not found", body = ProblemDetails)
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth" = ["document:read"])),
    tag = "Documents"
)]
pub async fn get_document(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DocumentResponse>, ProblemDetails> {
    auth.require_scope("document:read")?;
    let doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("document not found"))?;
    enforce_permission(&state.pool, &auth, id, Permission::Read).await?;
    Ok(Json(doc.into()))
}

/// Search for documents by metadata or list them.
///
/// If 'q' is provided, it performs a JSONB containment search against document metadata.
/// Otherwise, it returns a paginated list of all documents.
#[utoipa::path(
    get,
    path = "/documents",
    responses(
        (status = 200, description = "List of documents", body = [DocumentResponse]),
        (status = 400, description = "Bad request", body = ProblemDetails),
        (status = 401, description = "Unauthorized", body = ProblemDetails)
    ),
    params(
        ("q" = Option<String>, Query, description = "JSON metadata search pattern"),
        ("limit" = Option<i64>, Query, description = "Page size"),
        ("offset" = Option<i64>, Query, description = "Page offset")
    ),
    security(("bearerAuth"=["document:read"])),
    tag = "Documents"
)]
pub async fn search_documents(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Query(p): Query<SearchQuery>,
) -> Result<Json<Vec<DocumentResponse>>, ProblemDetails> {
    auth.require_scope("document:read")?;
    let limit = p.effective_limit();
    let offset = p.effective_offset();
    let rows: Vec<Document> = if let Some(q) = p.q {
        let pattern = serde_json::from_str::<serde_json::Value>(&q)
            .map_err(|e| bad_request(format!("invalid JSON in 'q' parameter: {e}")))?;
        sqlx::query_as::<_, Document>("SELECT id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at, deleted_at, deleted_by, archived_at, parent_id FROM documents WHERE metadata @> $1::jsonb AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT $2 OFFSET $3")
            .bind(pattern).bind(limit).bind(offset)
            .fetch_all(&state.pool).await.map_err(internal)?
    } else {
        DocumentRepo::list(&state.pool, limit, offset)
            .await
            .map_err(internal)?
    };
    // Filter results to only documents the caller has at least read permission on.
    let user_roles: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(roles) FROM users WHERE id = $1",
    )
    .bind(auth.user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;
    let doc_ids: Vec<Uuid> = rows.iter().map(|d| d.id).collect();
    let readable = AclService::filter_readable(&state.pool, auth.user_id, &user_roles, &doc_ids)
        .await
        .map_err(internal)?;
    let filtered: Vec<DocumentResponse> = rows
        .into_iter()
        .filter(|d| readable.contains(&d.id))
        .map(Into::into)
        .collect();
    Ok(Json(filtered))
}

/// Update document metadata.
///
/// Allows partial updates to the document shell (title and metadata).
#[utoipa::path(
    patch,
    path = "/documents/{id}",
    request_body = PatchDocumentRequest,
    responses(
        (status = 200, description = "Document updated successfully", body = DocumentResponse),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden", body = ProblemDetails),
        (status = 404, description = "Not found", body = ProblemDetails)
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["document:write"])),
    tag = "Documents"
)]
pub async fn patch_document(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
    Json(req): Json<PatchDocumentRequest>,
) -> Result<Json<DocumentResponse>, ProblemDetails> {
    auth.require_scope("document:write")?;
    enforce_permission(&state.pool, &auth, id, Permission::Write).await?;
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("document not found"))?;
    if let Some(ref t) = req.title {
        validate_title(t)?;
    }
    if let Some(t) = req.title {
        doc.title = t;
    }
    if let Some(m) = req.metadata {
        doc.metadata = m;
    }
    doc.updated_at = Utc::now();
    let mut tx = state.pool.begin().await.map_err(internal)?;
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog::builder(auth.user_id, "document.update", "document", id).build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}

/// Change document lifecycle status.
///
/// Moves a document through its lifecycle. The allowed transitions are:
/// - **draft → active** — publish the document for use.
/// - **active → archived** — move the document to long-term storage.
/// - **archived → active** — reactivate an archived document.
/// - **draft / active / archived → deleted** — soft-delete the document.
///
/// Soft-deleting a document does **not** remove any data. The document and its
/// versions remain available for restore. However, deletion is **blocked** if the
/// document is under legal hold or has an active retention period that has not yet
/// expired.
///
/// Once soft-deleted, a document may be restored via the restore endpoint, or it
/// will eventually be permanently purged by a background worker (see purge rules).
#[utoipa::path(
    post,
    path = "/documents/{id}/status",
    request_body = StatusChangeRequest,
    responses(
        (status = 200, description = "Status updated successfully", body = DocumentResponse),
        (status = 400, description = "Bad request", body = ProblemDetails),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden", body = ProblemDetails),
        (status = 404, description = "Not found", body = ProblemDetails)
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["document:write"])),
    tag = "Documents"
)]
pub async fn change_status(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
    Json(req): Json<StatusChangeRequest>,
) -> Result<Json<DocumentResponse>, ProblemDetails> {
    auth.require_scope("document:write")?;
    enforce_permission(&state.pool, &auth, id, Permission::Write).await?;
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("document not found"))?;

    // Validate transition
    let valid = matches!(
        (doc.status.as_str(), req.status.as_str()),
        ("draft", "active")
            | ("active", "archived")
            | ("archived", "active")
            | ("draft", "deleted")
            | ("active", "deleted")
            | ("archived", "deleted")
    );
    if !valid {
        return Err(bad_request("invalid transition"));
    }

    // Protect against deletion if under legal hold or retention
    if req.status == doc_status::DELETED && doc.is_protected() {
        return Err(bad_request(
            "cannot delete: document is under legal hold or retention",
        ));
    }

    let now = Utc::now();
    doc.status = req.status.clone();
    doc.updated_at = now;

    match req.status.as_str() {
        "deleted" => {
            doc.deleted_at = Some(now);
            doc.deleted_by = Some(auth.user_id);
        }
        "archived" => {
            doc.archived_at = Some(now);
        }
        "active" if doc.archived_at.is_some() => {
            doc.archived_at = None;
        }
        _ => {}
    }

    let mut tx = state.pool.begin().await.map_err(internal)?;
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog::builder(auth.user_id, "document.status", "document", id)
        .with_details(serde_json::json!({"status": doc.status}))
        .build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}

/// Restore a deleted or archived document.
///
/// - **Deleted** documents are restored to `draft` status (deletion metadata is cleared).
/// - **Archived** documents are restored to `active` status (archive metadata is cleared).
///
/// Once a document has been permanently purged, it **cannot** be restored.
#[utoipa::path(
    post,
    path = "/documents/{id}/restore",
    responses(
        (status = 200, description = "Document restored", body = DocumentResponse),
        (status = 400, description = "Bad request", body = ProblemDetails),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden", body = ProblemDetails),
        (status = 404, description = "Not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["document:write"])),
    tag = "Documents"
)]
pub async fn restore_document(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DocumentResponse>, ProblemDetails> {
    auth.require_scope("document:write")?;
    enforce_permission(&state.pool, &auth, id, Permission::Write).await?;
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("document not found"))?;

    if doc.status != doc_status::DELETED && doc.status != doc_status::ARCHIVED {
        return Err(bad_request("document is not deleted or archived"));
    }

    let now = Utc::now();
    match doc.status.as_str() {
        "deleted" => {
            doc.status = doc_status::DRAFT.to_string();
            doc.deleted_at = None;
            doc.deleted_by = None;
        }
        "archived" => {
            doc.status = doc_status::ACTIVE.to_string();
            doc.archived_at = None;
        }
        _ => unreachable!(),
    }
    doc.updated_at = now;

    let mut tx = state.pool.begin().await.map_err(internal)?;
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog::builder(auth.user_id, "document.restore", "document", id).build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}

/// Set or clear legal hold on a document.
///
/// When legal hold is enabled, the document is **fully protected**: it cannot be
/// soft-deleted by users and it is excluded from automatic purge by the background
/// worker — regardless of its retention date or current status.
///
/// Legal hold must be explicitly lifted (set to `false`) before the document can
/// be deleted or purged. Only administrators can manage legal hold.
#[utoipa::path(
    post,
    path = "/documents/{id}/legal-hold",
    request_body = LegalHoldRequest,
    responses(
        (status = 200, description = "Legal hold updated", body = DocumentResponse),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden", body = ProblemDetails),
        (status = 404, description = "Not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["admin"])),
    tag = "Documents"
)]
pub async fn set_legal_hold(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
    Json(req): Json<LegalHoldRequest>,
) -> Result<Json<DocumentResponse>, ProblemDetails> {
    if !auth.has_scope("admin") {
        return Err(forbidden("requires admin role"));
    }
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("document not found"))?;

    doc.legal_hold = req.hold;
    doc.updated_at = Utc::now();

    let mut tx = state.pool.begin().await.map_err(internal)?;
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog::builder(auth.user_id, "document.legal_hold", "document", id)
        .with_details(serde_json::json!({"legal_hold": req.hold}))
        .build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}

/// Set or clear retention period on a document.
///
/// A retention date defines the earliest point at which a document may be deleted
/// or purged. While the retention period is active, soft-deletion is blocked.
///
/// **Important:** once the retention date passes, the document becomes eligible for
/// automatic purge by the background worker — even if it was never soft-deleted.
/// To prevent automatic purge after retention expires, either clear the retention
/// date or place the document under legal hold.
///
/// Only administrators can manage retention policies.
#[utoipa::path(
    post,
    path = "/documents/{id}/retention",
    request_body = RetentionRequest,
    responses(
        (status = 200, description = "Retention updated", body = DocumentResponse),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden", body = ProblemDetails),
        (status = 404, description = "Not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["admin"])),
    tag = "Documents"
)]
pub async fn set_retention(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
    Json(req): Json<RetentionRequest>,
) -> Result<Json<DocumentResponse>, ProblemDetails> {
    if !auth.has_scope("admin") {
        return Err(forbidden("requires admin role"));
    }
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("document not found"))?;

    doc.retention_until = req.retention_until;
    doc.updated_at = Utc::now();

    let mut tx = state.pool.begin().await.map_err(internal)?;
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog::builder(auth.user_id, "document.retention", "document", id)
        .with_details(serde_json::json!({"retention_until": req.retention_until}))
        .build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}
