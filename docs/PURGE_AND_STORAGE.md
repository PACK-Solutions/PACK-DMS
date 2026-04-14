# Purge & Storage Management

This document describes how PackDMS manages blob storage, background purge
workers, orphan cleanup, and storage reconciliation.

---

## 1. Storage Architecture

PackDMS separates **document metadata** (PostgreSQL) from **binary content**
(blob storage). The blob storage backend is abstracted behind the `BlobStore`
trait, supporting S3-compatible and local filesystem implementations.

### Storage Key Format

Blob storage keys are **opaque** and UUID-based — original filenames are never
used as storage keys:

```
tenant/{owner_id}/blobs/{blob_id}
```

This avoids filename collisions, path traversal issues, and encoding problems.

### Content-Addressable Deduplication

Each uploaded file is hashed with **SHA-256**. The hash is stored alongside the
blob record for integrity verification. Blobs are tracked with a `ref_count`
that indicates how many version records reference them.

---

## 2. Blob Lifecycle

| Status             | Description                                        |
|--------------------|----------------------------------------------------|
| `active`           | In use by at least one version (`ref_count ≥ 1`)   |
| `pending_deletion` | Marked for removal (`ref_count = 0`)               |
| `purged`           | Permanently deleted from storage and database       |

When a version is soft-deleted, the blob's `ref_count` is decremented. When
`ref_count` reaches zero, the blob transitions to `pending_deletion`.

---

## 3. Background Workers

PackDMS runs four background workers as Tokio tasks, spawned at application
startup via `workers::spawn_all()`.

### 3.1 Blob Purge Worker

- **Interval:** every 30 seconds
- **Batch size:** 50 blobs per pass
- **Logic:**
  1. Query blobs with `status = 'pending_deletion'`
  2. Delete the object from blob storage (`BlobStore::delete`)
  3. On success: mark the blob as `purged` in the database
  4. On failure: mark as `purge_failed` for retry on the next pass

### 3.2 Document Retention Purge Worker

- **Interval:** every 60 seconds
- **Batch size:** 50 documents per pass
- **Logic:**
  1. Query documents eligible for purge (soft-deleted with expired retention,
     not under legal hold)
  2. For each document:
     - Soft-delete all active versions and decrement their blob `ref_count`
     - Mark the document as permanently purged
     - Create a system audit log entry (`document.retention_purge`)

### 3.3 Orphan Cleanup Worker

- **Interval:** every 5 minutes
- **Logic:** Processes `cleanup_orphans` jobs from the job outbox
- **Status:** Placeholder — full S3 listing reconciliation is not yet
  implemented. Jobs are acknowledged and completed without action.

### 3.4 Reconciliation Worker

- **Interval:** every hour
- **Batch size:** 100 blobs per pass
- **Logic:**
  1. Query active blobs from the database
  2. For each blob, check if the object exists in storage (`BlobStore::head`)
  3. **Size mismatch** → log a warning audit entry
     (`reconciliation.size_mismatch`)
  4. **Missing from storage** → mark the blob as `pending_deletion` and log
     an audit entry (`reconciliation.missing_blob`)

---

## 4. Job Outbox

Background tasks that need reliable execution use the `job_outbox` table:

| Field          | Description                                    |
|----------------|------------------------------------------------|
| `id`           | Unique job identifier                          |
| `job_type`     | Type of job (`purge_blob`, `cleanup_orphans`, `reconcile`) |
| `payload`      | JSON payload with job-specific data            |
| `status`       | `pending` → `processing` → `completed` / `failed` / `dead` |
| `attempts`     | Number of processing attempts                  |
| `max_attempts` | Maximum retries before marking as `dead`       |
| `last_error`   | Error message from the most recent failure     |
| `completed_at` | Timestamp of successful completion             |

Jobs are claimed atomically (`claim_next`) to prevent duplicate processing
across multiple worker instances.

---

## 5. Deletion Flow Summary

```
User soft-deletes version
        │
        ▼
Version.status → "deleted"
Blob.ref_count -= 1
        │
        ▼ (if ref_count = 0)
Blob.status → "pending_deletion"
        │
        ▼ (blob purge worker)
Storage object deleted
Blob.status → "purged"
```

For document-level purge (retention expiry):

```
Document retention expires
        │
        ▼ (document purge worker)
All active versions soft-deleted
Blob ref_counts decremented
Document marked as purged
Audit log: "document.retention_purge"
        │
        ▼ (blob purge worker, next pass)
Orphaned blobs deleted from storage
```

---

## 6. Protection Guards

The purge workers respect the same protection rules as the API:

- **Legal hold** — documents with `legal_hold = true` are never purged
- **Active retention** — documents with `retention_until > now` are skipped
- Only documents that are both soft-deleted **and** unprotected are eligible
  for automatic purge
