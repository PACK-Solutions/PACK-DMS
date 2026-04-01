use crate::domain::models::{AuditLog, Blob, Document, DocumentAcl, DocumentVersion, JobOutbox, User};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub struct DocumentRepo;

impl DocumentRepo {
    pub async fn create(tx: &mut Transaction<'_, Postgres>, doc: &Document) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO documents (id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at, deleted_at, deleted_by, archived_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"
        )
        .bind(doc.id)
        .bind(&doc.title)
        .bind(&doc.status)
        .bind(doc.owner_id)
        .bind(doc.current_version_id)
        .bind(doc.legal_hold)
        .bind(doc.retention_until)
        .bind(&doc.metadata)
        .bind(doc.created_at)
        .bind(doc.updated_at)
        .bind(doc.deleted_at)
        .bind(doc.deleted_by)
        .bind(doc.archived_at)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Document>> {
        sqlx::query_as::<_, Document>(
            "SELECT id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at, deleted_at, deleted_by, archived_at FROM documents WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn update(tx: &mut Transaction<'_, Postgres>, doc: &Document) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE documents SET title = $2, status = $3, owner_id = $4, current_version_id = $5, legal_hold = $6, retention_until = $7, metadata = $8, updated_at = $9, deleted_at = $10, deleted_by = $11, archived_at = $12 WHERE id = $1"
        )
        .bind(doc.id)
        .bind(&doc.title)
        .bind(&doc.status)
        .bind(doc.owner_id)
        .bind(doc.current_version_id)
        .bind(doc.legal_hold)
        .bind(doc.retention_until)
        .bind(&doc.metadata)
        .bind(doc.updated_at)
        .bind(doc.deleted_at)
        .bind(doc.deleted_by)
        .bind(doc.archived_at)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn list(pool: &PgPool, limit: i64, offset: i64) -> sqlx::Result<Vec<Document>> {
        sqlx::query_as::<_, Document>(
            "SELECT id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at, deleted_at, deleted_by, archived_at FROM documents WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }

    /// List documents that are soft-deleted and eligible for purge.
    pub async fn list_pending_purge(pool: &PgPool, limit: i64) -> sqlx::Result<Vec<Document>> {
        sqlx::query_as::<_, Document>(
            "SELECT id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at, deleted_at, deleted_by, archived_at \
             FROM documents WHERE status = 'deleted' AND deleted_at IS NOT NULL AND legal_hold = false \
             AND (retention_until IS NULL OR retention_until < NOW()) \
             ORDER BY deleted_at ASC LIMIT $1"
        )
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}

pub struct VersionRepo;

impl VersionRepo {
    pub async fn create(
        tx: &mut Transaction<'_, Postgres>,
        version: &DocumentVersion,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO document_versions (id, document_id, version_number, created_by, storage_key, content_hash, size_bytes, mime_type, created_at, status, original_filename, deleted_at, deleted_by, blob_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"
        )
        .bind(version.id)
        .bind(version.document_id)
        .bind(version.version_number)
        .bind(version.created_by)
        .bind(&version.storage_key)
        .bind(&version.content_hash)
        .bind(version.size_bytes)
        .bind(&version.mime_type)
        .bind(version.created_at)
        .bind(&version.status)
        .bind(&version.original_filename)
        .bind(version.deleted_at)
        .bind(version.deleted_by)
        .bind(version.blob_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn list_by_document_id(
        pool: &PgPool,
        document_id: Uuid,
    ) -> sqlx::Result<Vec<DocumentVersion>> {
        sqlx::query_as::<_, DocumentVersion>(
            "SELECT id, document_id, version_number, created_by, storage_key, content_hash, size_bytes, mime_type, created_at, status, original_filename, deleted_at, deleted_by, blob_id \
             FROM document_versions WHERE document_id = $1 ORDER BY version_number DESC"
        )
        .bind(document_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<DocumentVersion>> {
        sqlx::query_as::<_, DocumentVersion>(
            "SELECT id, document_id, version_number, created_by, storage_key, content_hash, size_bytes, mime_type, created_at, status, original_filename, deleted_at, deleted_by, blob_id \
             FROM document_versions WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// Mark a version as soft-deleted.
    pub async fn soft_delete(
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
        deleted_by: Uuid,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE document_versions SET status = 'deleted', deleted_at = NOW(), deleted_by = $2 WHERE id = $1"
        )
        .bind(id)
        .bind(deleted_by)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Mark previous active versions as superseded when a new version becomes current.
    pub async fn supersede_previous(
        tx: &mut Transaction<'_, Postgres>,
        document_id: Uuid,
        current_version_id: Uuid,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE document_versions SET status = 'superseded' WHERE document_id = $1 AND id != $2 AND status = 'active'"
        )
        .bind(document_id)
        .bind(current_version_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Restore a soft-deleted version.
    pub async fn restore(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE document_versions SET status = 'active', deleted_at = NULL, deleted_by = NULL WHERE id = $1"
        )
        .bind(id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

pub struct BlobRepo;

impl BlobRepo {
    pub async fn create(tx: &mut Transaction<'_, Postgres>, blob: &Blob) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO blobs (id, storage_key, content_hash, size_bytes, mime_type, ref_count, status, created_at, purged_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(blob.id)
        .bind(&blob.storage_key)
        .bind(&blob.content_hash)
        .bind(blob.size_bytes)
        .bind(&blob.mime_type)
        .bind(blob.ref_count)
        .bind(&blob.status)
        .bind(blob.created_at)
        .bind(blob.purged_at)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Blob>> {
        sqlx::query_as::<_, Blob>(
            "SELECT id, storage_key, content_hash, size_bytes, mime_type, ref_count, status, created_at, purged_at FROM blobs WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// Decrement ref_count and mark as pending_deletion if it reaches 0.
    pub async fn decrement_ref(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE blobs SET ref_count = GREATEST(ref_count - 1, 0) WHERE id = $1"
        )
        .bind(id)
        .execute(&mut **tx)
        .await?;
        // Mark for deletion if no more references
        sqlx::query(
            "UPDATE blobs SET status = 'pending_deletion' WHERE id = $1 AND ref_count = 0 AND status = 'active'"
        )
        .bind(id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Increment ref_count (e.g., on restore or dedup).
    pub async fn increment_ref(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("UPDATE blobs SET ref_count = ref_count + 1, status = 'active' WHERE id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// List blobs pending physical deletion.
    pub async fn list_pending_deletion(pool: &PgPool, limit: i64) -> sqlx::Result<Vec<Blob>> {
        sqlx::query_as::<_, Blob>(
            "SELECT id, storage_key, content_hash, size_bytes, mime_type, ref_count, status, created_at, purged_at \
             FROM blobs WHERE status = 'pending_deletion' AND ref_count = 0 ORDER BY created_at ASC LIMIT $1"
        )
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    /// Mark a blob as purged after successful physical deletion.
    pub async fn mark_purged(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("UPDATE blobs SET status = 'purged', purged_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    /// Mark a blob as purge_failed.
    pub async fn mark_purge_failed(
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> sqlx::Result<()> {
        sqlx::query("UPDATE blobs SET status = 'purge_failed' WHERE id = $1")
            .bind(id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}

pub struct JobRepo;

impl JobRepo {
    pub async fn create(tx: &mut Transaction<'_, Postgres>, job: &JobOutbox) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO job_outbox (id, job_type, payload, status, attempts, max_attempts, scheduled_at, started_at, completed_at, last_error, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
        )
        .bind(job.id)
        .bind(&job.job_type)
        .bind(&job.payload)
        .bind(&job.status)
        .bind(job.attempts)
        .bind(job.max_attempts)
        .bind(job.scheduled_at)
        .bind(job.started_at)
        .bind(job.completed_at)
        .bind(&job.last_error)
        .bind(job.created_at)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Claim the next pending job (atomic: sets status to processing).
    pub async fn claim_next(pool: &PgPool, job_type: &str) -> sqlx::Result<Option<JobOutbox>> {
        sqlx::query_as::<_, JobOutbox>(
            "UPDATE job_outbox SET status = 'processing', started_at = NOW(), attempts = attempts + 1 \
             WHERE id = (SELECT id FROM job_outbox WHERE status = 'pending' AND job_type = $1 AND scheduled_at <= NOW() ORDER BY scheduled_at ASC LIMIT 1 FOR UPDATE SKIP LOCKED) \
             RETURNING id, job_type, payload, status, attempts, max_attempts, scheduled_at, started_at, completed_at, last_error, created_at"
        )
        .bind(job_type)
        .fetch_optional(pool)
        .await
    }

    /// Mark a job as completed.
    pub async fn complete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("UPDATE job_outbox SET status = 'completed', completed_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Mark a job as failed. If max attempts reached, mark as dead.
    pub async fn fail(pool: &PgPool, id: Uuid, error: &str) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE job_outbox SET status = CASE WHEN attempts >= max_attempts THEN 'dead' ELSE 'pending' END, \
             last_error = $2, scheduled_at = NOW() + INTERVAL '30 seconds' WHERE id = $1"
        )
        .bind(id)
        .bind(error)
        .execute(pool)
        .await?;
        Ok(())
    }
}

pub struct AuditRepo;

impl AuditRepo {
    pub async fn create(tx: &mut Transaction<'_, Postgres>, log: &AuditLog) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO audit_log (id, ts, actor_id, action, resource_type, resource_id, version_id, request_id, ip, user_agent, outcome, details) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::inet, $10, $11, $12)"
        )
        .bind(log.id)
        .bind(log.ts)
        .bind(log.actor_id)
        .bind(&log.action)
        .bind(&log.resource_type)
        .bind(log.resource_id)
        .bind(log.version_id)
        .bind(&log.request_id)
        .bind(&log.ip)
        .bind(&log.user_agent)
        .bind(&log.outcome)
        .bind(&log.details)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn list_by_resource(pool: &PgPool, resource_id: Uuid) -> sqlx::Result<Vec<AuditLog>> {
        sqlx::query_as::<_, AuditLog>(
            "SELECT id, ts, actor_id, action, resource_type, resource_id, version_id, request_id, ip::text as ip, user_agent, outcome, details FROM audit_log WHERE resource_id = $1 ORDER BY ts DESC"
        )
        .bind(resource_id)
        .fetch_all(pool)
        .await
    }

    pub async fn list_all(pool: &PgPool, limit: i64, offset: i64) -> sqlx::Result<Vec<AuditLog>> {
        sqlx::query_as::<_, AuditLog>(
            "SELECT id, ts, actor_id, action, resource_type, resource_id, version_id, request_id, ip::text as ip, user_agent, outcome, details FROM audit_log ORDER BY ts DESC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
    }
}

pub struct AclRepo;

impl AclRepo {
    pub async fn create(tx: &mut Transaction<'_, Postgres>, acl: &DocumentAcl) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO document_acl (id, document_id, principal_type, principal_id, role, permission) \
             VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(acl.id)
        .bind(acl.document_id)
        .bind(&acl.principal_type)
        .bind(acl.principal_id)
        .bind(&acl.role)
        .bind(&acl.permission)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn list_by_document_id(
        pool: &PgPool,
        document_id: Uuid,
    ) -> sqlx::Result<Vec<DocumentAcl>> {
        sqlx::query_as::<_, DocumentAcl>(
            "SELECT id, document_id, principal_type, principal_id, role, permission FROM document_acl WHERE document_id = $1"
        )
        .bind(document_id)
        .fetch_all(pool)
        .await
    }

    pub async fn delete_by_document_id(
        tx: &mut Transaction<'_, Postgres>,
        document_id: Uuid,
    ) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM document_acl WHERE document_id = $1")
            .bind(document_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}

pub struct UserRepo;

impl UserRepo {
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<User>> {
        sqlx::query_as::<_, User>(
            "SELECT id, email, roles, status, created_at FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_email(pool: &PgPool, email: &str) -> sqlx::Result<Option<User>> {
        sqlx::query_as::<_, User>(
            "SELECT id, email, roles, status, created_at FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(pool)
        .await
    }
}
