use crate::domain::models::*;
use crate::infra::{
    auth::{AppState, JwtAuth},
    db::{AclRepo, AuditRepo, DocumentRepo, VersionRepo},
};
use axum::{
    extract::{Path, Query, State}, http::StatusCode,
    routing::{get, post},
    Json,
    Router,
};
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use axum::extract::Multipart;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

/// Request to create a new document.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "title": "Employee Contract",
    "metadata": {"contract_id": "CON-123", "department": "Legal", "confidential": true}
}))]
pub struct CreateDocumentRequest {
    /// Title of the document.
    pub title: String,
    /// Initial metadata associated with the document.
    #[schema(value_type = Object, example = json!({"contract_id": "CON-123", "department": "Legal", "confidential": true}))]
    pub metadata: serde_json::Value,
}

/// Detailed document information returned by the API.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "id": "00000000-0000-0000-0000-000000000000",
    "title": "Employee Contract",
    "status": "draft",
    "owner_id": "00000000-0000-0000-0000-000000000001",
    "current_version_id": null,
    "metadata": {"contract_id": "CON-123", "department": "Legal", "confidential": true},
    "created_at": "2024-01-01T00:00:00Z",
    "updated_at": "2024-01-01T00:00:00Z"
}))]
pub struct DocumentResponse {
    /// Unique identifier of the document.
    pub id: Uuid,
    /// Title of the document.
    pub title: String,
    /// Current status (draft, active, archived, deleted).
    pub status: String,
    /// User ID of the document owner.
    pub owner_id: Uuid,
    /// Identifier of the latest version (null if no content uploaded yet).
    pub current_version_id: Option<Uuid>,
    /// Metadata associated with the document.
    #[schema(value_type = Object, example = json!({"contract_id": "CON-123", "department": "Legal", "confidential": true}))]
    pub metadata: serde_json::Value,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<Document> for DocumentResponse {
    fn from(d: Document) -> Self {
        Self {
            id: d.id,
            title: d.title,
            status: d.status,
            owner_id: d.owner_id,
            current_version_id: d.current_version_id,
            metadata: d.metadata,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

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
    if !auth.has_scope("document:write") {
        return Err((StatusCode::FORBIDDEN, "Missing document:write scope".into()));
    }
    let mut tx = state.pool.begin().await.map_err(internal)?;
    let now = Utc::now();
    let doc = Document {
        id: Uuid::new_v4(),
        title: req.title,
        status: "draft".to_string(),
        owner_id: auth.user_id,
        current_version_id: None,
        legal_hold: false,
        retention_until: None,
        metadata: req.metadata,
        created_at: now,
        updated_at: now,
    };
    DocumentRepo::create(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog {
        id: Uuid::new_v4(),
        ts: now,
        actor_id: Some(auth.user_id),
        action: "document.create".into(),
        resource_type: "document".into(),
        resource_id: doc.id,
        version_id: None,
        request_id: None,
        ip: None,
        user_agent: None,
        outcome: "success".into(),
        details: serde_json::json!({}),
    };
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
    if !auth.has_scope("document:read") {
        return Err((StatusCode::FORBIDDEN, "Missing document:read scope".into()));
    }
    // AuthZ policy: for MVP skip ACL enforcement here or add later
    let Some(doc) = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "not found".into()));
    };
    Ok(Json(doc.into()))
}

/// Multipart request to upload a new document version.
#[derive(ToSchema)]
pub struct UploadVersionRequest {
    /// The document file to be stored.
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

/// Brief information about the created version.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct VersionResponse {
    /// Unique identifier for the new version.
    pub id: Uuid,
    /// The assigned version number.
    pub version_number: i32,
    /// Timestamp of the upload.
    pub created_at: chrono::DateTime<Utc>,
}

/// Upload a new version of the document content.
///
/// This endpoint accepts multipart/form-data. The 'file' part should contain the binary content.
/// The document status must allow version updates.
#[utoipa::path(
    post,
    path = "/documents/{id}/versions",
    request_body(content = UploadVersionRequest, content_type = "multipart/form-data"),
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
) -> Result<(StatusCode, Json<VersionResponse>), (StatusCode, String)> {
    if !auth.has_scope("document:write") {
        return Err((StatusCode::FORBIDDEN, "Missing document:write scope".into()));
    }
    let Some(mut doc) = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "not found".into()));
    };

    let mut data = None;
    let mut mime_type = "application/octet-stream".to_string();

    while let Some(field) = multipart.next_field().await.map_err(internal)? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            if let Some(content_type) = field.content_type() {
                mime_type = content_type.to_string();
            }
            data = Some(field.bytes().await.map_err(internal)?);
        }
    }

    let data = data.ok_or((StatusCode::BAD_REQUEST, "missing file part".into()))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hex::encode(hasher.finalize());
    let bytes = Bytes::from(data);
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
    let storage_key = format!("{}/{}.bin", id, vnum);
    let size_bytes = bytes.len() as i64;
    state
        .storage
        .put(&storage_key, bytes.clone())
        .await
        .map_err(internal)?;
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
    };
    // Start transaction only when we are ready to write to DB to avoid pool exhaustion in tests with low max connections
    let mut tx = state.pool.begin().await.map_err(internal)?;
    VersionRepo::create(&mut tx, &version)
        .await
        .map_err(internal)?;
    doc.current_version_id = Some(vid);
    doc.updated_at = Utc::now();
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog {
        id: Uuid::new_v4(),
        ts: Utc::now(),
        actor_id: Some(auth.user_id),
        action: "version.upload".into(),
        resource_type: "document".into(),
        resource_id: id,
        version_id: Some(vid),
        request_id: None,
        ip: None,
        user_agent: None,
        outcome: "success".into(),
        details: serde_json::json!({"size": version.size_bytes}),
    };
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
) -> Result<Json<Vec<DocumentVersion>>, (StatusCode, String)> {
    if !auth.has_scope("document:read") {
        return Err((StatusCode::FORBIDDEN, "Missing document:read scope".into()));
    }
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
) -> Result<(StatusCode, Vec<u8>), (StatusCode, String)> {
    if !auth.has_scope("document:read") {
        return Err((StatusCode::FORBIDDEN, "Missing document:read scope".into()));
    }
    let Some(v) = VersionRepo::find_by_id(&state.pool, vid)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "not found".into()));
    };
    if v.document_id != id {
        return Err((StatusCode::NOT_FOUND, "not found".into()));
    }
    let bytes = state.storage.get(&v.storage_key).await.map_err(internal)?;
    Ok((StatusCode::OK, bytes.to_vec()))
}

/// Query parameters for searching documents and audit logs.
#[derive(Deserialize)]
pub struct SearchQuery {
    /// JSON pattern to match against document metadata (e.g., {"department": "Legal"}).
    pub q: Option<String>,
    /// Maximum number of records to return (default: 50).
    pub limit: Option<i64>,
    /// Number of records to skip (default: 0).
    pub offset: Option<i64>,
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
    if !auth.has_scope("document:read") {
        return Err((StatusCode::FORBIDDEN, "Missing document:read scope".into()));
    }
    let limit = p.limit.unwrap_or(50);
    let offset = p.offset.unwrap_or(0);
    // Very simple: if q provided, filter jsonb contains
    let rows: Vec<Document> = if let Some(q) = p.q {
        let pattern =
            serde_json::from_str::<serde_json::Value>(&q).unwrap_or(serde_json::json!({}));
        sqlx::query_as::<_, Document>("SELECT id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at FROM documents WHERE metadata @> $1::jsonb ORDER BY updated_at DESC LIMIT $2 OFFSET $3")
            .bind(pattern).bind(limit).bind(offset)
            .fetch_all(&state.pool).await.map_err(internal)?
    } else {
        DocumentRepo::list(&state.pool, limit, offset)
            .await
            .map_err(internal)?
    };
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// Request to update document fields.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "title": "Updated title",
    "metadata": {"contract_id": "CON-123", "department": "Legal"}
}))]
pub struct PatchDocumentRequest {
    /// Optional new title.
    pub title: Option<String>,
    /// Optional new metadata (replaces existing metadata).
    #[schema(value_type = Object, example = json!({"contract_id": "CON-123", "department": "Legal"}))]
    pub metadata: Option<serde_json::Value>,
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
    if !auth.has_scope("document:write") {
        return Err((StatusCode::FORBIDDEN, "Missing document:write scope".into()));
    }
    let Some(mut doc) = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "not found".into()));
    };
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
    let audit = AuditLog {
        id: Uuid::new_v4(),
        ts: Utc::now(),
        actor_id: Some(auth.user_id),
        action: "document.update".into(),
        resource_type: "document".into(),
        resource_id: id,
        version_id: None,
        request_id: None,
        ip: None,
        user_agent: None,
        outcome: "success".into(),
        details: serde_json::json!({}),
    };
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}

/// Request to change a document's status.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct StatusChangeRequest {
    /// New status (draft, active, archived, deleted).
    #[schema(example = "active")]
    pub status: String,
}

/// Change document lifecycle status.
///
/// Implements state transition rules:
/// - draft -> active
/// - active -> archived
/// - archived -> active
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
    if !auth.has_scope("document:write") {
        return Err((StatusCode::FORBIDDEN, "Missing document:write scope".into()));
    }
    let Some(mut doc) = DocumentRepo::find_by_id(&state.pool, id)
        .await
        .map_err(internal)?
    else {
        return Err((StatusCode::NOT_FOUND, "not found".into()));
    };
    // Lifecycle rules simplified
    match (doc.status.as_str(), req.status.as_str()) {
        ("draft", "active") | ("active", "archived") | ("archived", "active") => {}
        _ => return Err((StatusCode::BAD_REQUEST, "invalid transition".into())),
    };
    if !auth.has_role("admin") && doc.owner_id != auth.user_id {
        return Err((StatusCode::FORBIDDEN, "forbidden".into()));
    }
    doc.status = req.status;
    doc.updated_at = Utc::now();
    let mut tx = state.pool.begin().await.map_err(internal)?;
    DocumentRepo::update(&mut tx, &doc)
        .await
        .map_err(internal)?;
    let audit = AuditLog {
        id: Uuid::new_v4(),
        ts: Utc::now(),
        actor_id: Some(auth.user_id),
        action: "document.status".into(),
        resource_type: "document".into(),
        resource_id: id,
        version_id: None,
        request_id: None,
        ip: None,
        user_agent: None,
        outcome: "success".into(),
        details: serde_json::json!({"status": doc.status}),
    };
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(Json(doc.into()))
}

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
) -> Result<Json<Vec<DocumentAcl>>, (StatusCode, String)> {
    if !auth.has_scope("document:read") {
        return Err((StatusCode::FORBIDDEN, "Missing document:read scope".into()));
    }
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
) -> Result<StatusCode, (StatusCode, String)> {
    if !auth.has_scope("document:write") {
        return Err((StatusCode::FORBIDDEN, "Missing document:write scope".into()));
    }
    if !auth.has_role("admin") {
        return Err((StatusCode::FORBIDDEN, "forbidden".into()));
    }
    let mut tx = state.pool.begin().await.map_err(internal)?;
    AclRepo::delete_by_document_id(&mut tx, id)
        .await
        .map_err(internal)?;
    for mut r in rules.into_iter() {
        r.id = Uuid::new_v4();
        r.document_id = id;
        AclRepo::create(&mut tx, &r).await.map_err(internal)?;
    }
    let audit = AuditLog {
        id: Uuid::new_v4(),
        ts: Utc::now(),
        actor_id: Some(auth.user_id),
        action: "document.acl".into(),
        resource_type: "document".into(),
        resource_id: id,
        version_id: None,
        request_id: None,
        ip: None,
        user_agent: None,
        outcome: "success".into(),
        details: serde_json::json!({}),
    };
    AuditRepo::create(&mut tx, &audit).await.map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

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
) -> Result<Json<Vec<AuditLog>>, (StatusCode, String)> {
    if !auth.has_role("admin") {
        return Err((StatusCode::FORBIDDEN, "Missing admin role".into()));
    }
    let logs = AuditRepo::list_all(&state.pool, p.limit.unwrap_or(100), p.offset.unwrap_or(0))
        .await
        .map_err(internal)?;
    Ok(Json(logs))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_document,
        get_document,
        upload_version,
        list_versions,
        download_version,
        search_documents,
        patch_document,
        change_status,
        get_acl,
        put_acl,
        list_audit
    ),
    components(
        schemas(
            CreateDocumentRequest,
            DocumentResponse,
            UploadVersionRequest,
            VersionResponse,
            Document,
            DocumentVersion,
            DocumentAcl,
            AuditLog,
            DocumentStatus
        )
    ),
    modifiers(&SecurityAddon),
    info(
        title = "PackDMS API",
        version = "0.1.0",
        description = "A secure Document Management System API supporting versioning, metadata search, and audit logging."
    )
)]
pub struct ApiDoc;

pub struct SecurityAddon;
impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
        use utoipa::openapi::{security::SecurityRequirement, Components};
        let scheme = SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer));
        let components = openapi.components.get_or_insert_with(Components::default);
        components.add_security_scheme("bearerAuth", scheme);
        openapi.security = Some(vec![SecurityRequirement::new::<_, Vec<String>, _>(
            "bearerAuth",
            Vec::<String>::new(),
        )]);
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/documents", post(create_document).get(search_documents))
        .route("/documents/{id}", get(get_document).patch(patch_document))
        .route("/documents/{id}/status", post(change_status))
        .route(
            "/documents/{id}/versions",
            post(upload_version).get(list_versions),
        )
        .route(
            "/documents/{id}/versions/{vid}/download",
            get(download_version),
        )
        .route("/documents/{id}/acl", get(get_acl).put(put_acl))
        .with_state(state)
}

fn internal<E: std::fmt::Debug>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:?}"))
}
