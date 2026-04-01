-- =============================================================================
-- Migration: Document Lifecycle Management
-- Adds version status, soft delete, purge tracking, blob deduplication support,
-- and enriches the document lifecycle state machine.
-- =============================================================================

-- 1. Add version-level lifecycle status to document_versions
ALTER TABLE document_versions
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active',
    ADD COLUMN IF NOT EXISTS original_filename TEXT NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS deleted_by UUID REFERENCES users(id);

-- 2. Add soft-delete and enriched lifecycle fields to documents
ALTER TABLE documents
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS deleted_by UUID REFERENCES users(id),
    ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

-- 3. Blob registry: physical objects in S3, decoupled from versions
-- Enables deduplication and safe purge (only purge when ref_count = 0)
CREATE TABLE IF NOT EXISTS blobs (
    id UUID PRIMARY KEY,
    storage_key TEXT UNIQUE NOT NULL,
    content_hash TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    mime_type TEXT NOT NULL,
    ref_count INT NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',  -- active, pending_deletion, purged, purge_failed
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    purged_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_blobs_content_hash ON blobs(content_hash);
CREATE INDEX IF NOT EXISTS idx_blobs_status ON blobs(status);

-- 4. Link document_versions to blobs
ALTER TABLE document_versions
    ADD COLUMN IF NOT EXISTS blob_id UUID REFERENCES blobs(id);

-- 5. Outbox table for async jobs (upload finalization, purge, reconciliation)
CREATE TABLE IF NOT EXISTS job_outbox (
    id UUID PRIMARY KEY,
    job_type TEXT NOT NULL,          -- e.g. 'purge_blob', 'finalize_upload', 'reconcile', 'cleanup_orphans'
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, processing, completed, failed, dead
    attempts INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 3,
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_job_outbox_status_scheduled ON job_outbox(status, scheduled_at);
CREATE INDEX IF NOT EXISTS idx_job_outbox_type_status ON job_outbox(job_type, status);

-- 6. Update existing version rows: populate original_filename from storage_key
-- Extract filename from storage_key pattern: tenant/{owner}/document/{id}/v{n}/original/{filename}
UPDATE document_versions
SET original_filename = COALESCE(
    NULLIF(regexp_replace(storage_key, '^.*/original/', ''), ''),
    'unknown'
)
WHERE original_filename = 'unknown';

-- 7. Index for soft-deleted documents
CREATE INDEX IF NOT EXISTS idx_documents_deleted_at ON documents(deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_document_versions_status ON document_versions(status);
