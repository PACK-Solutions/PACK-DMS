# Document Versioning

This document describes how PackDMS manages document versions, content uploads,
downloads, and content integrity.

---

## 1. Overview

PackDMS uses an **immutable versioning** model. Each upload creates a new
version — existing versions are never overwritten. This provides a complete
audit trail of every change to a document's content.

---

## 2. Version Numbering

Versions are numbered with a **monotonically increasing integer** starting at 1.
Each new upload increments the version number by one. Version numbers are never
reused, even if earlier versions are deleted.

---

## 3. Upload Flow

```
Client uploads file (multipart/form-data)
        │
        ▼
Server computes SHA-256 hash
        │
        ▼
Blob stored at: tenant/{owner_id}/blobs/{blob_id}
        │
        ▼
Transaction:
  1. Create blob record (storage_key, hash, size, mime_type, ref_count=1)
  2. Mark previous active version as "superseded"
  3. Create new version record (version_number, blob_id, filename, etc.)
  4. Update document.current_version_id
  5. Create audit log entry
  6. Commit
```

### Upload Constraints

- Only documents in `draft` or `active` status accept new versions
- The `file` part in the multipart request is required
- Original filename and MIME type are preserved as metadata
- Requires `document:write` scope and `write` ACL permission

### API

```
POST /documents/{id}/versions
Content-Type: multipart/form-data

file: <binary content>
```

Returns **201 Created** with the version metadata.

---

## 4. Version Statuses

| Status       | Description                                          |
|--------------|------------------------------------------------------|
| `active`     | The current version; set on the most recent upload   |
| `superseded` | Replaced by a newer version; still downloadable      |
| `deleted`    | Soft-deleted; excluded from listings and downloads   |

---

## 5. Listing Versions

```
GET /documents/{id}/versions
```

Returns all non-deleted versions for the document, ordered by version number.
Requires `document:read` scope and `read` ACL permission.

---

## 6. Downloading a Version

```
GET /documents/{id}/versions/{vid}/download
```

- Returns the binary content with appropriate `Content-Type` and
  `Content-Disposition` headers
- The original filename is included in the `Content-Disposition` header
  (sanitized to prevent header injection)
- Deleted versions return **404 Not Found**
- Requires `document:read` scope and `read` ACL permission

---

## 7. Deleting a Version

```
DELETE /documents/{id}/versions/{vid}
```

- Performs a **soft-delete** — the version record is retained but marked as
  `deleted` with `deleted_at` and `deleted_by` timestamps
- The blob's `ref_count` is decremented; when it reaches zero, the blob
  becomes eligible for purge by the background worker
- Deletion is **blocked** if the parent document is under legal hold or has
  an active retention period
- Already-deleted versions return **400 Bad Request**
- Returns **204 No Content** on success
- Requires `document:write` scope and `write` ACL permission

---

## 8. Content Integrity

Every uploaded file is hashed with **SHA-256** at upload time. The hash is
stored in both the version record (`content_hash`) and the blob record. This
enables:

- **Integrity verification** — detect corruption or tampering
- **Reconciliation** — the background reconciliation worker compares stored
  blob sizes against database records to detect inconsistencies

---

## 9. Blob Storage Separation

Versions reference blobs indirectly via `blob_id`. The blob table tracks:

- `storage_key` — the opaque path in blob storage
- `ref_count` — number of version records referencing this blob
- `status` — lifecycle state (`active`, `pending_deletion`, `purged`)

This separation allows the system to manage storage cleanup independently
from version metadata, and supports future content deduplication (multiple
versions sharing the same blob when content is identical).

---

## 10. Filename Sanitization

Original filenames are preserved for user convenience but are **sanitized**
before use in HTTP headers. The following characters are stripped:

- Double quotes (`"`)
- Backslashes (`\`)
- Newlines and carriage returns (`\n`, `\r`)
- Null bytes (`\0`)

This prevents HTTP header injection attacks via crafted filenames.
