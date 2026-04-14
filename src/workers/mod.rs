//! Background workers for async document lifecycle operations.
//!
//! Workers are spawned as Tokio tasks and poll the `job_outbox` table
//! or run periodic maintenance (purge, reconciliation, orphan cleanup).

use crate::domain::models::AuditLog;
use crate::infra::db::{AuditRepo, BlobRepo, DocumentRepo, JobRepo};
use crate::infra::storage::BlobStore;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

/// Spawn all background workers. Returns join handles for graceful shutdown.
pub fn spawn_all(pool: PgPool, storage: Arc<dyn BlobStore>) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();

    // Blob purge worker — processes pending_deletion blobs
    {
        let pool = pool.clone();
        let storage = storage.clone();
        handles.push(tokio::spawn(async move {
            blob_purge_loop(pool, storage).await;
        }));
    }

    // Document retention purge worker — purges documents past retention
    {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            document_purge_loop(pool).await;
        }));
    }

    // Orphan cleanup worker — cleans up abandoned uploads
    {
        let pool = pool.clone();
        let storage = storage.clone();
        handles.push(tokio::spawn(async move {
            orphan_cleanup_loop(pool, storage).await;
        }));
    }

    // Reconciliation worker — detects DB/storage inconsistencies
    {
        let pool = pool.clone();
        let storage = storage.clone();
        handles.push(tokio::spawn(async move {
            reconciliation_loop(pool, storage).await;
        }));
    }

    handles
}

/// Periodically purge blobs marked as pending_deletion with ref_count = 0.
async fn blob_purge_loop(pool: PgPool, storage: Arc<dyn BlobStore>) {
    let interval = Duration::from_secs(30);
    loop {
        tokio::time::sleep(interval).await;
        if let Err(e) = run_blob_purge(&pool, &storage).await {
            tracing::error!("blob purge error: {e:?}");
        }
    }
}

/// Single pass of blob purge.
pub async fn run_blob_purge(pool: &PgPool, storage: &Arc<dyn BlobStore>) -> anyhow::Result<()> {
    let blobs = BlobRepo::list_pending_deletion(pool, 50).await?;
    for blob in blobs {
        tracing::info!(blob_id = %blob.id, key = %blob.storage_key, "purging blob from storage");
        match storage.delete(&blob.storage_key).await {
            Ok(()) => {
                let mut tx = pool.begin().await?;
                BlobRepo::mark_purged(&mut tx, blob.id).await?;
                tx.commit().await?;
                tracing::info!(blob_id = %blob.id, "blob purged successfully");
            }
            Err(e) => {
                tracing::error!(blob_id = %blob.id, "blob purge failed: {e:?}");
                let mut tx = pool.begin().await?;
                BlobRepo::mark_purge_failed(&mut tx, blob.id).await?;
                tx.commit().await?;
            }
        }
    }
    Ok(())
}

/// Periodically purge documents whose retention period has expired.
async fn document_purge_loop(pool: PgPool) {
    let interval = Duration::from_secs(60);
    loop {
        tokio::time::sleep(interval).await;
        if let Err(e) = run_document_purge(&pool).await {
            tracing::error!("document retention purge error: {e:?}");
        }
    }
}

/// Single pass of document retention purge.
pub async fn run_document_purge(pool: &PgPool) -> anyhow::Result<()> {
    let documents = DocumentRepo::list_pending_purge(pool, 50).await?;
    for doc in documents {
        tracing::info!(
            document_id = %doc.id,
            title = %doc.title,
            retention_until = ?doc.retention_until,
            "purging document after retention expiry"
        );
        let mut tx = pool.begin().await?;

        // Soft-delete associated versions and decrement blob ref_counts
        let versions =
            crate::infra::db::VersionRepo::list_by_document_id(pool, doc.id).await?;
        for version in &versions {
            if version.status != "deleted" {
                crate::infra::db::VersionRepo::soft_delete(
                    &mut tx,
                    version.id,
                    doc.owner_id,
                )
                .await?;
                // Only decrement for versions not already deleted — already-deleted
                // versions had their blob ref_count decremented at delete time.
                if let Some(blob_id) = version.blob_id {
                    BlobRepo::decrement_ref(&mut tx, blob_id).await?;
                }
            }
        }

        DocumentRepo::mark_purged(&mut tx, doc.id).await?;
        let audit = AuditLog::system_builder(
            "document.retention_purge",
            "document",
            doc.id,
        )
        .with_details(serde_json::json!({
            "title": doc.title,
            "retention_until": doc.retention_until,
            "deleted_at": doc.deleted_at,
        }))
        .build();
        AuditRepo::create(&mut tx, &audit).await?;
        tx.commit().await?;
        tracing::info!(document_id = %doc.id, "document purged successfully");
    }
    Ok(())
}

/// Periodically process jobs from the outbox.
async fn orphan_cleanup_loop(pool: PgPool, storage: Arc<dyn BlobStore>) {
    let interval = Duration::from_secs(300); // every 5 minutes
    loop {
        tokio::time::sleep(interval).await;
        if let Err(e) = run_orphan_cleanup(&pool, &storage).await {
            tracing::error!("orphan cleanup error: {e:?}");
        }
    }
}

/// Clean up blobs that exist in storage but have no DB reference (orphans).
/// Also processes outbox jobs of type 'cleanup_orphans'.
pub async fn run_orphan_cleanup(
    pool: &PgPool,
    _storage: &Arc<dyn BlobStore>,
) -> anyhow::Result<()> {
    // Process any pending cleanup_orphans jobs
    while let Some(job) = JobRepo::claim_next(pool, "cleanup_orphans").await? {
        tracing::warn!(
            job_id = %job.id,
            "orphan cleanup not yet implemented — skipping job"
        );
        // TODO: Implement full S3 listing reconciliation to detect and
        // remove blobs that exist in storage but have no DB reference.
        JobRepo::complete(pool, job.id).await?;
    }
    Ok(())
}

/// Periodically check DB/storage consistency.
async fn reconciliation_loop(pool: PgPool, storage: Arc<dyn BlobStore>) {
    let interval = Duration::from_secs(3600); // every hour
    loop {
        tokio::time::sleep(interval).await;
        if let Err(e) = run_reconciliation(&pool, &storage).await {
            tracing::error!("reconciliation error: {e:?}");
        }
    }
}

/// Verify that active blobs in DB actually exist in storage.
pub async fn run_reconciliation(pool: &PgPool, storage: &Arc<dyn BlobStore>) -> anyhow::Result<()> {
    // Check a batch of active blobs
    let blobs: Vec<crate::domain::models::Blob> = sqlx::query_as(
        "SELECT id, storage_key, content_hash, size_bytes, mime_type, ref_count, status, created_at, purged_at \
         FROM blobs WHERE status = 'active' ORDER BY created_at ASC LIMIT 100"
    )
    .fetch_all(pool)
    .await?;

    for blob in blobs {
        match storage.head(&blob.storage_key).await {
            Ok(Some(size)) => {
                if size != blob.size_bytes {
                    tracing::warn!(
                        blob_id = %blob.id,
                        expected = blob.size_bytes,
                        actual = size,
                        "blob size mismatch detected"
                    );
                    let mut tx = pool.begin().await?;
                    let audit = AuditLog::system_builder(
                        "reconciliation.size_mismatch",
                        "blob",
                        blob.id,
                    )
                    .with_details(serde_json::json!({
                        "storage_key": blob.storage_key,
                        "expected_size": blob.size_bytes,
                        "actual_size": size,
                    }))
                    .build();
                    AuditRepo::create(&mut tx, &audit).await?;
                    tx.commit().await?;
                }
            }
            Ok(None) => {
                tracing::error!(
                    blob_id = %blob.id,
                    key = %blob.storage_key,
                    "blob missing from storage — DB/storage inconsistency"
                );
                let mut tx = pool.begin().await?;
                let audit = AuditLog::system_builder(
                    "reconciliation.missing_blob",
                    "blob",
                    blob.id,
                )
                .with_details(serde_json::json!({
                    "storage_key": blob.storage_key,
                    "size_bytes": blob.size_bytes,
                }))
                .build();
                AuditRepo::create(&mut tx, &audit).await?;
                tx.commit().await?;
            }
            Err(e) => {
                tracing::warn!(blob_id = %blob.id, "reconciliation check failed: {e:?}");
            }
        }
    }
    Ok(())
}
