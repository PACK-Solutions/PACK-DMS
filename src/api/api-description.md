# PackDMS — Document Management System API

PackDMS is a secure, API-first document management system built with Rust. It provides full document lifecycle management, binary content versioning, access control, and audit logging.

## Authentication

All endpoints require a **Bearer JWT** token in the `Authorization` header. Tokens must be RS256-signed and contain:
- `sub` — user UUID
- `scope` — space-separated scopes (e.g. `document:read document:write admin`)
- `iss` — issuer matching the server configuration

## Document Lifecycle

Documents follow a strict state machine:

```
  ┌───────┐    activate    ┌────────┐    archive    ┌──────────┐
  │ Draft │───────────────▶│ Active │──────────────▶│ Archived │
  └───┬───┘                └───┬────┘               └─────┬────┘
      │                        │                          │
      │                        │    restore               │
      │                        │◀─────────────────────────┘
      │                        │
      │  soft-delete           │  soft-delete          soft-delete
      ▼                        ▼                          │
  ┌─────────┐◀────────────────────────────────────────────┘
  │ Deleted │
  └────┬────┘
       │  restore
       ▼
  ┌───────┐
  │ Draft │
  └───────┘
```

Once purged by the background retention worker, a document reaches the **terminal `purged` state** and cannot be restored.

### Status transitions

| From | To | Conditions |
|---|---|---|
| `draft` | `active` | At least one version must exist |
| `active` | `archived` | Always allowed |
| `archived` | `active` | Always allowed |
| `draft` | `deleted` | Soft-delete |
| `active` | `deleted` | Not under legal hold or active retention |
| `archived` | `deleted` | Not under legal hold or active retention |
| `deleted` | `draft` | Restore operation |

## Versioning

Each document can have multiple **versions** of its binary content. Versions are uploaded as `multipart/form-data` and stored in blob storage (S3-compatible or local filesystem). Each version records:
- Auto-incremented version number
- Original filename and MIME type
- SHA-256 checksum
- File size
- Upload timestamp

The document's `current_version` pointer is automatically updated on each upload.

## Protection Mechanisms

### Legal Hold
A document under legal hold **cannot be deleted or have its status changed to deleted**. Legal hold is toggled via a dedicated endpoint and is recorded in the audit log.

### Retention Policy
A retention period sets an expiry date before which the document **cannot be deleted**. Once the retention date passes, normal deletion rules apply again. Additionally, **when the retention date expires, the document is automatically purged** by a background worker — regardless of whether it was soft-deleted first.

### Automatic Purge

A background worker runs every 60 seconds and permanently purges documents that meet **either** of these conditions (provided `legal_hold` is `false`):

1. **Soft-deleted documents** whose optional `retention_until` has expired (or was never set).
2. **Any non-purged document** (including `active`, `draft`, or `archived`) whose `retention_until` date has passed.

When a document is purged:
- All associated versions are soft-deleted.
- Blob reference counts are decremented; blobs reaching `ref_count = 0` are marked `pending_deletion`.
- The document status is set to `purged` (a terminal state).
- An audit log entry (`document.retention_purge`) is created.

A separate **blob purge worker** (every 30 s) then deletes the physical objects from storage for blobs in `pending_deletion` status.

> **Note:** Purged documents cannot be restored. To prevent automatic purge, either remove the `retention_until` date before it expires or place the document under legal hold.

## Access Control (ACL)

Each document has an Access Control List with entries granting specific permissions to users:
- `read` — view document metadata and download versions
- `write` — update metadata, upload versions, change status
- `admin` — manage ACL entries

ACLs are replaced atomically via a PUT operation.

## Audit Logging

Every significant action is recorded in an immutable audit log, including:
- Document creation, updates, status changes
- Version uploads and deletions
- ACL modifications
- Legal hold and retention changes

Audit entries capture the action type, actor (user UUID), timestamp, and a detail payload.

## Error Handling

All errors are returned as **RFC 9457 Problem Details** JSON objects (`application/problem+json`) with fields: `type`, `title`, `status`, `detail`, and `instance`.

## Admin Frontend

A built-in administration UI is available at `/admin` for interactive document management, preview, and audit log browsing.
