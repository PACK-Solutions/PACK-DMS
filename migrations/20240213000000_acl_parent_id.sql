-- =============================================================================
-- Add parent_id to documents for folder/collection hierarchy and ACL inheritance.
-- =============================================================================

ALTER TABLE documents ADD COLUMN parent_id UUID REFERENCES documents(id);

CREATE INDEX IF NOT EXISTS idx_documents_parent_id ON documents(parent_id) WHERE parent_id IS NOT NULL;
