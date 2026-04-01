use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// Represents the current lifecycle status of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
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

impl DocumentStatus {
    /// Returns `true` if transitioning from `self` to `target` is allowed.
    pub fn can_transition_to(&self, target: &DocumentStatus) -> bool {
        matches!(
            (self, target),
            (DocumentStatus::Draft, DocumentStatus::Active)
                | (DocumentStatus::Active, DocumentStatus::Archived)
                | (DocumentStatus::Archived, DocumentStatus::Active)
                | (DocumentStatus::Active, DocumentStatus::Deleted)
                | (DocumentStatus::Draft, DocumentStatus::Deleted)
                | (DocumentStatus::Archived, DocumentStatus::Deleted)
        )
    }
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
    /// Timestamp when the document was soft-deleted (None if not deleted).
    pub deleted_at: Option<DateTime<Utc>>,
    /// User who performed the soft-delete.
    pub deleted_by: Option<Uuid>,
    /// Timestamp when the document was archived.
    pub archived_at: Option<DateTime<Utc>>,
}

impl Document {
    /// Returns `true` if the document is under retention lock.
    pub fn is_retention_locked(&self) -> bool {
        self.retention_until
            .map(|r| r > Utc::now())
            .unwrap_or(false)
    }

    /// Returns `true` if the document cannot be deleted or purged.
    pub fn is_protected(&self) -> bool {
        self.legal_hold || self.is_retention_locked()
    }
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
    /// Lifecycle status of this version (active, superseded, deleted).
    pub status: String,
    /// Original filename as provided by the uploader.
    pub original_filename: String,
    /// Timestamp when this version was soft-deleted.
    pub deleted_at: Option<DateTime<Utc>>,
    /// User who soft-deleted this version.
    pub deleted_by: Option<Uuid>,
    /// Reference to the physical blob in the blobs table.
    pub blob_id: Option<Uuid>,
}

/// Version lifecycle status values.
pub mod version_status {
    pub const ACTIVE: &str = "active";
    pub const SUPERSEDED: &str = "superseded";
    pub const DELETED: &str = "deleted";
}

/// Document lifecycle status values (string form matching DB).
pub mod doc_status {
    pub const DRAFT: &str = "draft";
    pub const ACTIVE: &str = "active";
    pub const ARCHIVED: &str = "archived";
    pub const DELETED: &str = "deleted";
}

/// A physical blob stored in S3/RustFS.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromRow)]
pub struct Blob {
    /// Unique identifier for this blob.
    pub id: Uuid,
    /// Opaque storage key in S3.
    pub storage_key: String,
    /// SHA-256 content hash.
    pub content_hash: String,
    /// Size in bytes.
    pub size_bytes: i64,
    /// MIME type.
    pub mime_type: String,
    /// Number of versions referencing this blob.
    pub ref_count: i32,
    /// Blob status: active, pending_deletion, purged, purge_failed.
    pub status: String,
    /// When the blob was registered.
    pub created_at: DateTime<Utc>,
    /// When the blob was physically purged from storage.
    pub purged_at: Option<DateTime<Utc>>,
}

/// Blob status values.
pub mod blob_status {
    pub const ACTIVE: &str = "active";
    pub const PENDING_DELETION: &str = "pending_deletion";
    pub const PURGED: &str = "purged";
    pub const PURGE_FAILED: &str = "purge_failed";
}

/// An async job entry in the outbox.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobOutbox {
    pub id: Uuid,
    pub job_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Job status values.
pub mod job_status {
    pub const PENDING: &str = "pending";
    pub const PROCESSING: &str = "processing";
    pub const COMPLETED: &str = "completed";
    pub const FAILED: &str = "failed";
    pub const DEAD: &str = "dead";
}

/// Job type constants.
pub mod job_type {
    pub const PURGE_BLOB: &str = "purge_blob";
    pub const CLEANUP_ORPHANS: &str = "cleanup_orphans";
    pub const RECONCILE: &str = "reconcile";
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

/// Builder for constructing `AuditLog` entries with sensible defaults.
pub struct AuditLogBuilder {
    actor_id: Uuid,
    action: String,
    resource_type: String,
    resource_id: Uuid,
    version_id: Option<Uuid>,
    details: serde_json::Value,
}

impl AuditLogBuilder {
    /// Set the related version identifier.
    pub fn with_version(mut self, version_id: Uuid) -> Self {
        self.version_id = Some(version_id);
        self
    }

    /// Set additional details for the audit entry.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }

    /// Consume the builder and produce the final `AuditLog`.
    pub fn build(self) -> AuditLog {
        AuditLog {
            id: Uuid::new_v4(),
            ts: Utc::now(),
            actor_id: Some(self.actor_id),
            action: self.action,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            version_id: self.version_id,
            request_id: None,
            ip: None,
            user_agent: None,
            outcome: "success".into(),
            details: self.details,
        }
    }
}

impl AuditLog {
    /// Start building a new audit log entry.
    pub fn builder(
        actor_id: Uuid,
        action: &str,
        resource_type: &str,
        resource_id: Uuid,
    ) -> AuditLogBuilder {
        AuditLogBuilder {
            actor_id,
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id,
            version_id: None,
            details: serde_json::json!({}),
        }
    }
}
