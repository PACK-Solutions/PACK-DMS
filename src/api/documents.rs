use crate::domain::models::*;
use crate::infra::{
    auth::{AppState, JwtAuth},
    db::{AuditRepo, DocumentRepo},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use super::error::internal;
use super::types::{
    CreateDocumentRequest, DocumentResponse, LegalHoldRequest, PatchDocumentRequest,
    RetentionRequest, SearchQuery, StatusChangeRequest,
};

/// Create a new document shell.
///
/// Documents are created in 'draft' status with no initial content.
/// Use the versions endpoint to upload the actual document file.
#[utoipa::path(
    post,
    path = "/documents",
    security(("bearerAuth" = ["document:write"])),
    tag = "Documents"
)]
pub async fn create_document(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Json(req): Json<CreateDocumentRequest>,
) -> Result<(StatusCode, Json<DocumentResponse>), (StatusCode, String)> {
    auth.require_scope("document:write")?;
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
    };
    DocumentRepo::create(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog::builder(auth.user_id, "document.create", "document", doc.id)
        .build();
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
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Document not found")
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
) -> Result<Json<DocumentResponse>, (StatusCode, String)> {
    auth.require_scope("document:read")?;
    let doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "not found".into()))?;
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
        (status = 400, description = "Invalid JSON in 'q' parameter"),
        (status = 401, description = "Unauthorized")
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
) -> Result<Json<Vec<DocumentResponse>>, (StatusCode, String)> {
    auth.require_scope("document:read")?;
    let limit = p.limit.unwrap_or(50);
    let offset = p.offset.unwrap_or(0);
    let rows: Vec<Document> = if let Some(q) = p.q {
        let pattern = serde_json::from_str::<serde_json::Value>(&q).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid JSON in 'q' parameter: {e}"),
            )
        })?;
        sqlx::query_as::<_, Document>("SELECT id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at, deleted_at, deleted_by, archived_at FROM documents WHERE metadata @> $1::jsonb AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT $2 OFFSET $3")
            .bind(pattern).bind(limit).bind(offset)
            .fetch_all(&state.pool).await.map_err(internal)?
    } else {
        DocumentRepo::list(&state.pool, limit, offset)
            .await
            .map_err(internal)?
    };
    Ok(Json(rows.into_iter().map(Into::into).collect()))
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
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (not owner or admin)"),
        (status = 404, description = "Document not found")
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
) -> Result<Json<DocumentResponse>, (StatusCode, String)> {
    auth.require_scope("document:write")?;
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "not found".into()))?;
    if doc.owner_id != auth.user_id && !auth.has_role("admin") {
        return Err((StatusCode::FORBIDDEN, "forbidden".into()));
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
    let audit = AuditLog::builder(auth.user_id, "document.update", "document", id)
        .build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}

/// Change document lifecycle status.
///
/// Implements state transition rules:
/// - draft -> active
/// - active -> archived
/// - archived -> active
/// - draft -> deleted (soft delete)
/// - active -> deleted (soft delete, if not protected)
/// - archived -> deleted (soft delete, if not protected)
#[utoipa::path(
    post,
    path = "/documents/{id}/status",
    request_body = StatusChangeRequest,
    responses(
        (status = 200, description = "Status updated successfully", body = DocumentResponse),
        (status = 400, description = "Invalid status transition"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (not owner or admin)"),
        (status = 404, description = "Document not found")
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
) -> Result<Json<DocumentResponse>, (StatusCode, String)> {
    auth.require_scope("document:write")?;
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "not found".into()))?;

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
        return Err((StatusCode::BAD_REQUEST, "invalid transition".into()));
    }

    if !auth.has_role("admin") && doc.owner_id != auth.user_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".into()));
    }

    // Protect against deletion if under legal hold or retention
    if req.status == doc_status::DELETED && doc.is_protected() {
        return Err((
            StatusCode::BAD_REQUEST,
            "cannot delete: document is under legal hold or retention".into(),
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

/// Restore a soft-deleted document back to draft status.
#[utoipa::path(
    post,
    path = "/documents/{id}/restore",
    responses(
        (status = 200, description = "Document restored", body = DocumentResponse),
        (status = 400, description = "Document is not deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
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
) -> Result<Json<DocumentResponse>, (StatusCode, String)> {
    auth.require_scope("document:write")?;
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "not found".into()))?;

    if doc.status != doc_status::DELETED {
        return Err((StatusCode::BAD_REQUEST, "document is not deleted".into()));
    }

    if !auth.has_role("admin") && doc.owner_id != auth.user_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".into()));
    }

    let now = Utc::now();
    doc.status = doc_status::DRAFT.to_string();
    doc.deleted_at = None;
    doc.deleted_by = None;
    doc.updated_at = now;

    let mut tx = state.pool.begin().await.map_err(internal)?;
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog::builder(auth.user_id, "document.restore", "document", id)
        .build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}

/// Set or clear legal hold on a document.
#[utoipa::path(
    post,
    path = "/documents/{id}/legal-hold",
    request_body = LegalHoldRequest,
    responses(
        (status = 200, description = "Legal hold updated", body = DocumentResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (requires admin)"),
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
) -> Result<Json<DocumentResponse>, (StatusCode, String)> {
    if !auth.has_role("admin") {
        return Err((StatusCode::FORBIDDEN, "requires admin role".into()));
    }
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "not found".into()))?;

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
#[utoipa::path(
    post,
    path = "/documents/{id}/retention",
    request_body = RetentionRequest,
    responses(
        (status = 200, description = "Retention updated", body = DocumentResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (requires admin)"),
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
) -> Result<Json<DocumentResponse>, (StatusCode, String)> {
    if !auth.has_role("admin") {
        return Err((StatusCode::FORBIDDEN, "requires admin role".into()));
    }
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "not found".into()))?;

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
