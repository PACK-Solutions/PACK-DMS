use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// Represents the current lifecycle status of a document.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
pub enum DocumentStatus {
    /// Initial state, not yet published.
    Draft,
    /// Actively managed document.
    Active,
    /// Document preserved for long-term storage.
    Archived,
    /// Document marked for deletion but not yet purged.
    Deleted,
}

/// Represents a document's metadata and its current state.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
pub struct Document {
    /// Unique identifier for the document.
    pub id: Uuid,
    /// Human-readable title of the document.
    pub title: String,
    /// Current lifecycle status (e.g., draft, active).
    pub status: String,
    /// User ID of the document owner.
    pub owner_id: Uuid,
    /// Identifier of the most recent version, if any.
    pub current_version_id: Option<Uuid>,
    /// Whether the document is under legal hold (prevents deletion).
    pub legal_hold: bool,
    /// Date until which the document must be retained.
    pub retention_until: Option<DateTime<Utc>>,
    /// Flexible JSON metadata (e.g., contract IDs, departments).
    #[schema(value_type = Object, example = json!({"contract_id": "CON-123", "department": "Legal", "confidential": true}))]
    pub metadata: serde_json::Value,
    /// Timestamp when the document was first created.
    pub created_at: DateTime<Utc>,
    /// Timestamp of the last update to the document's metadata or status.
    pub updated_at: DateTime<Utc>,
}

/// A specific version of a document's content.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
pub struct DocumentVersion {
    /// Unique identifier for this version.
    pub id: Uuid,
    /// Reference to the parent document.
    pub document_id: Uuid,
    /// Monotonically increasing version number (starts at 1).
    pub version_number: i32,
    /// User ID of the person who uploaded this version.
    pub created_by: Uuid,
    /// Path or key used in the blob storage.
    pub storage_key: String,
    /// SHA-256 hash of the content for integrity verification.
    pub content_hash: String,
    /// Size of the content in bytes.
    pub size_bytes: i64,
    /// MIME type of the content (e.g., application/pdf).
    pub mime_type: String,
    /// Timestamp when this version was created.
    pub created_at: DateTime<Utc>,
}

/// Represents a system user.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
pub struct User {
    /// Unique identifier for the user.
    pub id: Uuid,
    /// Primary email address used for login.
    pub email: String,
    /// List of assigned roles (e.g., "user", "admin").
    pub roles: Vec<String>,
    /// User account status (e.g., "active", "disabled").
    pub status: String,
    /// Timestamp when the user account was created.
    pub created_at: DateTime<Utc>,
}

/// An Access Control List entry for a document.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
pub struct DocumentAcl {
    /// Unique identifier for this ACL entry.
    pub id: Uuid,
    /// Reference to the document being controlled.
    pub document_id: Uuid,
    /// Type of principal: "user" or "role".
    pub principal_type: String,
    /// Unique identifier of the user (if principal_type is "user").
    pub principal_id: Option<Uuid>,
    /// Name of the role (if principal_type is "role").
    pub role: Option<String>,
    /// Granted permission: "read", "write", or "admin".
    pub permission: String,
}

/// A record of an action performed within the system.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
pub struct AuditLog {
    /// Unique identifier for the audit record.
    pub id: Uuid,
    /// Timestamp when the event occurred.
    pub ts: DateTime<Utc>,
    /// User ID of the person performing the action.
    pub actor_id: Option<Uuid>,
    /// Action name (e.g., "document.create", "version.upload").
    pub action: String,
    /// Type of resource affected (e.g., "document").
    pub resource_type: String,
    /// Identifier of the affected resource.
    pub resource_id: Uuid,
    /// Identifier of the document version (if applicable).
    pub version_id: Option<Uuid>,
    /// Request ID from the HTTP tracing layer.
    pub request_id: Option<String>,
    /// Originating IP address of the request.
    pub ip: Option<String>,
    /// User agent string of the client.
    pub user_agent: Option<String>,
    /// Result of the action ("success" or "failure").
    pub outcome: String,
    /// Additional context or data about the action.
    pub details: serde_json::Value,
}
