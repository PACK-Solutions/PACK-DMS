use crate::domain::models::{AuditLog, Document, DocumentAcl, DocumentVersion, User};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub struct DocumentRepo;

impl DocumentRepo {
    pub async fn create(tx: &mut Transaction<'_, Postgres>, doc: &Document) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO documents (id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
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
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Document>> {
        sqlx::query_as::<_, Document>(
            "SELECT id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at FROM documents WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn update(tx: &mut Transaction<'_, Postgres>, doc: &Document) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE documents SET title = $2, status = $3, owner_id = $4, current_version_id = $5, legal_hold = $6, retention_until = $7, metadata = $8, updated_at = $9 WHERE id = $1"
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
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn list(pool: &PgPool, limit: i64, offset: i64) -> sqlx::Result<Vec<Document>> {
        sqlx::query_as::<_, Document>(
            "SELECT id, title, status, owner_id, current_version_id, legal_hold, retention_until, metadata, created_at, updated_at FROM documents ORDER BY updated_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
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
            "INSERT INTO document_versions (id, document_id, version_number, created_by, storage_key, content_hash, size_bytes, mime_type, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
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
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn list_by_document_id(
        pool: &PgPool,
        document_id: Uuid,
    ) -> sqlx::Result<Vec<DocumentVersion>> {
        sqlx::query_as::<_, DocumentVersion>(
            "SELECT id, document_id, version_number, created_by, storage_key, content_hash, size_bytes, mime_type, created_at FROM document_versions WHERE document_id = $1 ORDER BY version_number DESC"
        )
        .bind(document_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<DocumentVersion>> {
        sqlx::query_as::<_, DocumentVersion>(
            "SELECT id, document_id, version_number, created_by, storage_key, content_hash, size_bytes, mime_type, created_at FROM document_versions WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(pool)
        .await
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
