-- =============================================================================
-- PackDMS Schema
-- Core tables, lifecycle management, blob deduplication, and async job outbox.
-- =============================================================================

-- Create Users table
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    roles TEXT[] NOT NULL DEFAULT '{}',
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create Documents table
CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id),
    current_version_id UUID, -- Will be set after first version upload
    legal_hold BOOLEAN NOT NULL DEFAULT FALSE,
    retention_until TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    deleted_by UUID REFERENCES users(id),
    archived_at TIMESTAMPTZ,
    parent_id UUID REFERENCES documents(id)
);

-- Blob registry: physical objects in S3, decoupled from versions
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

-- Create Document Versions table
CREATE TABLE IF NOT EXISTS document_versions (
    id UUID PRIMARY KEY,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    version_number INT NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    storage_key TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    mime_type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status TEXT NOT NULL DEFAULT 'active',
    original_filename TEXT NOT NULL DEFAULT 'unknown',
    deleted_at TIMESTAMPTZ,
    deleted_by UUID REFERENCES users(id),
    blob_id UUID REFERENCES blobs(id),
    UNIQUE(document_id, version_number)
);

-- Add foreign key constraint to documents after document_versions is created
ALTER TABLE documents ADD CONSTRAINT fk_current_version FOREIGN KEY (current_version_id) REFERENCES document_versions(id);

-- Create Document ACL table
CREATE TABLE IF NOT EXISTS document_acl (
    id UUID PRIMARY KEY,
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    principal_type TEXT NOT NULL, -- 'user' or 'role'
    principal_id UUID, -- NULL if principal_type is 'role'
    role TEXT, -- NULL if principal_type is 'user'
    permission TEXT NOT NULL, -- 'read', 'write', 'admin'
    UNIQUE(document_id, principal_type, principal_id, role)
);

-- Create Audit Log table (append-only)
CREATE TABLE IF NOT EXISTS audit_log (
    id UUID PRIMARY KEY,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor_id UUID REFERENCES users(id),
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id UUID NOT NULL,
    version_id UUID REFERENCES document_versions(id),
    request_id TEXT,
    ip INET,
    user_agent TEXT,
    outcome TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonB
);

-- Outbox table for async jobs (upload finalization, purge, reconciliation)
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

-- Indexes
CREATE INDEX IF NOT EXISTS idx_documents_status_updated ON documents(status, updated_at);
CREATE INDEX IF NOT EXISTS idx_documents_owner_updated ON documents(owner_id, updated_at);
CREATE INDEX IF NOT EXISTS idx_documents_metadata ON documents USING GIN (metadata);
CREATE INDEX IF NOT EXISTS idx_documents_deleted_at ON documents(deleted_at) WHERE deleted_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_documents_parent_id ON documents(parent_id) WHERE parent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_document_versions_status ON document_versions(status);

CREATE INDEX IF NOT EXISTS idx_blobs_content_hash ON blobs(content_hash);
CREATE INDEX IF NOT EXISTS idx_blobs_status ON blobs(status);

CREATE INDEX IF NOT EXISTS idx_audit_log_resource_ts ON audit_log(resource_id, ts);
CREATE INDEX IF NOT EXISTS idx_audit_log_actor_ts ON audit_log(actor_id, ts);
CREATE INDEX IF NOT EXISTS idx_audit_log_action_ts ON audit_log(action, ts);

CREATE INDEX IF NOT EXISTS idx_job_outbox_status_scheduled ON job_outbox(status, scheduled_at);
CREATE INDEX IF NOT EXISTS idx_job_outbox_type_status ON job_outbox(job_type, status);

-- Seed some initial data
INSERT INTO users (id, email, roles, status) VALUES 
('00000000-0000-0000-0000-000000000001', 'admin@example.com', ARRAY['admin'], 'active'),
('00000000-0000-0000-0000-000000000002', 'user@example.com', ARRAY['user'], 'active');
