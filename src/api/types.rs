use crate::domain::models::Document;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
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
    "legal_hold": false,
    "retention_until": null,
    "metadata": {"contract_id": "CON-123", "department": "Legal", "confidential": true},
    "created_at": "2024-01-01T00:00:00Z",
    "updated_at": "2024-01-01T00:00:00Z",
    "deleted_at": null,
    "archived_at": null
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
    /// Whether the document is under legal hold.
    pub legal_hold: bool,
    /// Retention deadline (null if none).
    pub retention_until: Option<DateTime<Utc>>,
    /// Metadata associated with the document.
    #[schema(value_type = Object, example = json!({"contract_id": "CON-123", "department": "Legal", "confidential": true}))]
    pub metadata: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Soft-deletion timestamp (null if not deleted).
    pub deleted_at: Option<DateTime<Utc>>,
    /// Archive timestamp (null if not archived).
    pub archived_at: Option<DateTime<Utc>>,
    /// Optional parent document ID for folder/collection hierarchy.
    pub parent_id: Option<Uuid>,
}

impl From<Document> for DocumentResponse {
    fn from(d: Document) -> Self {
        Self {
            id: d.id,
            title: d.title,
            status: d.status,
            owner_id: d.owner_id,
            current_version_id: d.current_version_id,
            legal_hold: d.legal_hold,
            retention_until: d.retention_until,
            metadata: d.metadata,
            created_at: d.created_at,
            updated_at: d.updated_at,
            deleted_at: d.deleted_at,
            archived_at: d.archived_at,
            parent_id: d.parent_id,
        }
    }
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
    pub created_at: DateTime<Utc>,
}

/// Maximum number of records a single query may return.
const MAX_LIMIT: i64 = 200;
/// Default page size when the caller omits `limit`.
const DEFAULT_LIMIT: i64 = 50;

/// Maximum allowed length for a document title.
pub const MAX_TITLE_LENGTH: usize = 500;

/// Query parameters for searching documents and audit logs.
#[derive(Deserialize)]
pub struct SearchQuery {
    /// JSON pattern to match against document metadata (e.g., {"department": "Legal"}).
    pub q: Option<String>,
    /// Maximum number of records to return (default: 50, max: 200).
    pub limit: Option<i64>,
    /// Number of records to skip (default: 0).
    pub offset: Option<i64>,
}

impl SearchQuery {
    /// Return the effective limit, clamped to `[1, MAX_LIMIT]`.
    pub fn effective_limit(&self) -> i64 {
        self.limit
            .unwrap_or(DEFAULT_LIMIT)
            .clamp(1, MAX_LIMIT)
    }

    /// Return the effective offset, floored at 0.
    pub fn effective_offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
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

/// Request to change a document's status.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct StatusChangeRequest {
    /// New status (draft, active, archived, deleted).
    #[schema(example = "active")]
    pub status: String,
}

/// Request to set or clear legal hold.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct LegalHoldRequest {
    /// Whether to enable (true) or disable (false) legal hold.
    pub hold: bool,
}

/// Request to set or clear retention period.
#[derive(Serialize, Deserialize, ToSchema)]
pub struct RetentionRequest {
    /// Retention deadline. Set to null to clear.
    pub retention_until: Option<DateTime<Utc>>,
}
