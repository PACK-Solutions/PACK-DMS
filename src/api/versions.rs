use crate::domain::models::*;
use crate::infra::{
    auth::{AppState, JwtAuth},
    db::{AuditRepo, BlobRepo, DocumentRepo, VersionRepo},
};
use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::StatusCode,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use super::error::{ProblemDetails, bad_request, internal, not_found};
use super::types::VersionResponse;

/// Upload a new version of the document content.
///
/// This endpoint accepts multipart/form-data. The 'file' part should contain the binary content.
/// The document status must allow version updates.
/// Storage key is opaque (UUID-based); the original filename is preserved as metadata.
#[utoipa::path(
    post,
    path = "/documents/{id}/versions",
    request_body(content = super::types::UploadVersionRequest, content_type = "multipart/form-data"),
    responses(
        (status = 201, description = "Version uploaded successfully", body = VersionResponse),
        (status = 400, description = "Bad request (e.g., missing file)"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden (missing scope)"),
        (status = 404, description = "Document not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["document:write"])),
    tag = "Versions"
)]
pub async fn upload_version(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<VersionResponse>), ProblemDetails> {
    auth.require_scope("document:write")?;
    let mut doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("document not found"))?;

    // Only draft or active documents accept new versions
    if doc.status != doc_status::DRAFT && doc.status != doc_status::ACTIVE {
        return Err(bad_request("document status does not allow uploads"));
    }

    let mut data = None;
    let mut mime_type = "application/octet-stream".to_string();
    let mut filename = "blob".to_string();

    while let Some(field) = multipart.next_field().await.map_err(internal)? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            if let Some(content_type) = field.content_type() {
                mime_type = content_type.to_string();
            }
            if let Some(fname) = field.file_name() {
                filename = fname.to_string();
            }
            data = Some(field.bytes().await.map_err(internal)?);
        }
    }

    let data = data.ok_or_else(|| bad_request("missing file part"))?;

    // Compute SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hex::encode(hasher.finalize());

    // Determine next version number
    let vnum = match doc.current_version_id {
        Some(_) => {
            let versions = VersionRepo::list_by_document_id(&state.pool, id)
                .await
                .map_err(internal)?;
            versions.first().map(|v| v.version_number).unwrap_or(0) + 1
        }
        None => 1,
    };

    let vid = Uuid::new_v4();
    let blob_id = Uuid::new_v4();

    // Opaque storage key: tenant/{owner_id}/blobs/{blob_id}
    // No filename in the key — stable, scalable, non-ambiguous
    let storage_key = format!("tenant/{}/blobs/{}", doc.owner_id, blob_id);

    let size_bytes = data.len() as i64;

    // Upload to storage first (before DB commit)
    state
        .storage
        .put(&storage_key, data.clone(), Some(&mime_type))
        .await
        .map_err(internal)?;

    // Register blob and version in a single transaction
    let mut tx = state.pool.begin().await.map_err(internal)?;

    let blob = Blob {
        id: blob_id,
        storage_key: storage_key.clone(),
        content_hash: hash.clone(),
        size_bytes,
        mime_type: mime_type.clone(),
        ref_count: 1,
        status: blob_status::ACTIVE.to_string(),
        created_at: Utc::now(),
        purged_at: None,
    };
    BlobRepo::create(&mut tx, &blob).await.map_err(internal)?;

    let version = DocumentVersion {
        id: vid,
        document_id: id,
        version_number: vnum,
        created_by: auth.user_id,
        storage_key: storage_key.clone(),
        content_hash: hash,
        size_bytes,
        mime_type,
        created_at: Utc::now(),
        status: version_status::ACTIVE.to_string(),
        original_filename: filename,
        deleted_at: None,
        deleted_by: None,
        blob_id: Some(blob_id),
    };
    VersionRepo::create(&mut tx, &version)
        .await
        .map_err(internal)?;

    // Supersede previous active versions
    VersionRepo::supersede_previous(&mut tx, id, vid)
        .await
        .map_err(internal)?;

    doc.current_version_id = Some(vid);
    doc.updated_at = Utc::now();
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;

    let audit = AuditLog::builder(auth.user_id, "version.upload", "document", id)
        .with_version(vid)
        .with_details(serde_json::json!({
            "size": version.size_bytes,
            "version_number": vnum,
            "original_filename": version.original_filename,
            "content_hash": version.content_hash,
        }))
        .build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok((
        StatusCode::CREATED,
        Json(VersionResponse {
            id: vid,
            version_number: vnum,
            created_at: version.created_at,
        }),
    ))
}

/// List all versions of a document.
#[utoipa::path(
    get,
    path = "/documents/{id}/versions",
    responses(
        (status = 200, description = "List of versions", body = [DocumentVersion]),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Document not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier")
    ),
    security(("bearerAuth"=["document:read"])),
    tag = "Versions"
)]
pub async fn list_versions(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DocumentVersion>>, ProblemDetails> {
    auth.require_scope("document:read")?;
    let versions = VersionRepo::list_by_document_id(&state.pool, id)
        .await
        .map_err(internal)?;
    Ok(Json(versions))
}

/// Download a specific version of a document.
#[utoipa::path(
    get,
    path = "/documents/{id}/versions/{vid}/download",
    responses(
        (status = 200, description = "File content", body = Vec<u8>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Document or version not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier"),
        ("vid" = Uuid, Path, description = "Version identifier")
    ),
    security(("bearerAuth"=["document:read"])),
    tag = "Versions"
)]
pub async fn download_version(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path((id, vid)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, [(axum::http::header::HeaderName, String); 2], Vec<u8>), ProblemDetails> {
    auth.require_scope("document:read")?;
    let v = VersionRepo::find_by_id(&state.pool, vid)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("version not found"))?;
    if v.document_id != id {
        return Err(not_found("version not found"));
    }
    if v.status == version_status::DELETED {
        return Err(not_found("version deleted"));
    }
    let bytes = state.storage.get(&v.storage_key).await.map_err(internal)?;
    let headers = [
        (
            axum::http::header::CONTENT_TYPE,
            v.mime_type.clone(),
        ),
        (
            axum::http::header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", v.original_filename),
        ),
    ];
    Ok((StatusCode::OK, headers, bytes.to_vec()))
}

/// Soft-delete a specific version.
#[utoipa::path(
    delete,
    path = "/documents/{id}/versions/{vid}",
    responses(
        (status = 204, description = "Version soft-deleted"),
        (status = 400, description = "Cannot delete (protected)"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not found")
    ),
    params(
        ("id" = Uuid, Path, description = "Document identifier"),
        ("vid" = Uuid, Path, description = "Version identifier")
    ),
    security(("bearerAuth"=["document:write"])),
    tag = "Versions"
)]
pub async fn delete_version(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Path((id, vid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ProblemDetails> {
    auth.require_scope("document:write")?;
    let doc = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("document not found"))?;

    if doc.is_protected() {
        return Err(bad_request("document is under legal hold or retention"));
    }

    let v = VersionRepo::find_by_id(&state.pool, vid)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("version not found"))?;
    if v.document_id != id {
        return Err(not_found("version not found"));
    }
    if v.status == version_status::DELETED {
        return Err(bad_request("already deleted"));
    }

    let mut tx = state.pool.begin().await.map_err(internal)?;
    VersionRepo::soft_delete(&mut tx, vid, auth.user_id)
        .await
        .map_err(internal)?;

    // Decrement blob ref_count
    if let Some(blob_id) = v.blob_id {
        BlobRepo::decrement_ref(&mut tx, blob_id)
            .await
            .map_err(internal)?;
    }

    let audit = AuditLog::builder(auth.user_id, "version.delete", "document", id)
        .with_version(vid)
        .build();
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;

    Ok(StatusCode::NO_CONTENT)
}
