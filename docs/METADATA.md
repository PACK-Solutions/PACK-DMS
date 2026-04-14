# Document Metadata

This document describes how metadata is stored, queried, and managed in PackDMS.

---

## 1. Overview

Every document carries two kinds of metadata:

- **System fields** — managed by PackDMS (id, title, status, timestamps, etc.)
- **Custom metadata** — a free-form JSONB column for business-specific data

---

## 2. System Fields

| Field                | Type              | Description                                    |
|----------------------|-------------------|------------------------------------------------|
| `id`                 | UUID              | Unique document identifier                     |
| `title`              | String            | Human-readable title (required, max 500 chars) |
| `status`             | String            | Lifecycle status (`draft`, `active`, `archived`, `deleted`) |
| `owner_id`           | UUID              | User who created the document                  |
| `current_version_id` | UUID (nullable)   | Most recent version reference                  |
| `legal_hold`         | Boolean           | Whether the document is under legal hold       |
| `retention_until`    | Timestamp (nullable) | Retention expiry date                       |
| `metadata`           | JSONB             | Custom metadata (see below)                    |
| `created_at`         | Timestamp         | Creation time                                  |
| `updated_at`         | Timestamp         | Last modification time                         |
| `deleted_at`         | Timestamp (nullable) | Soft-deletion time                          |
| `deleted_by`         | UUID (nullable)   | User who soft-deleted the document             |
| `archived_at`        | Timestamp (nullable) | Archival time                               |
| `parent_id`          | UUID (nullable)   | Parent document for folder hierarchy           |

---

## 3. Custom Metadata (JSONB)

The `metadata` field is a **JSONB** column that accepts any valid JSON object.
This allows teams to attach domain-specific attributes without schema changes.

### Example

```json
{
  "contract_id": "CON-123",
  "department": "Legal",
  "confidential": true,
  "tags": ["nda", "2024"],
  "review_date": "2025-06-01"
}
```

### Setting Metadata

Metadata is set at creation time and can be updated via `PATCH /documents/{id}`:

```json
// POST /documents
{
  "title": "NDA Agreement",
  "metadata": { "contract_id": "CON-123", "department": "Legal" }
}

// PATCH /documents/{id}
{
  "metadata": { "contract_id": "CON-123", "department": "Legal", "confidential": true }
}
```

**Note:** `PATCH` replaces the entire `metadata` object — there is no deep
merge. To add a single key, include all existing keys in the request.

---

## 4. Metadata Search

The `GET /documents` endpoint supports JSONB containment search via the `q`
query parameter.

### How It Works

The `q` parameter accepts a JSON object. PackDMS uses PostgreSQL's `@>`
(containment) operator to find documents whose `metadata` contains all the
specified key-value pairs.

### Examples

```
# Find documents in the Legal department
GET /documents?q={"department":"Legal"}

# Find confidential documents with a specific contract
GET /documents?q={"contract_id":"CON-123","confidential":true}

# Find documents with a specific tag (exact array match)
GET /documents?q={"tags":["nda"]}
```

### Pagination

Results are paginated with `limit` and `offset` parameters:

```
GET /documents?q={"department":"Legal"}&limit=20&offset=40
```

Default limit is applied when not specified.

---

## 5. Title Validation

Document titles are validated on creation and update:

- Must not be empty (after trimming whitespace)
- Must not exceed **500 characters**
- Violations return **400 Bad Request**

---

## 6. Version Metadata

Each version also carries metadata managed by the system:

| Field               | Type      | Description                              |
|---------------------|-----------|------------------------------------------|
| `id`                | UUID      | Unique version identifier                |
| `document_id`       | UUID      | Parent document reference                |
| `version_number`    | Integer   | Monotonically increasing (starts at 1)   |
| `created_by`        | UUID      | User who uploaded the version            |
| `storage_key`       | String    | Opaque blob storage path                 |
| `content_hash`      | String    | SHA-256 hash of the file content         |
| `size_bytes`        | Integer   | File size in bytes                       |
| `mime_type`         | String    | MIME type (e.g., `application/pdf`)      |
| `original_filename` | String    | Filename as provided by the uploader     |
| `status`            | String    | Version status (`active`, `superseded`, `deleted`) |
| `created_at`        | Timestamp | Upload time                              |
| `deleted_at`        | Timestamp (nullable) | Soft-deletion time              |
| `deleted_by`        | UUID (nullable) | User who deleted the version        |
| `blob_id`           | UUID (nullable) | Reference to the physical blob      |

---

## 7. Audit Metadata

Audit log entries capture contextual metadata about every action:

| Field          | Type              | Description                          |
|----------------|-------------------|--------------------------------------|
| `actor_id`     | UUID (nullable)   | User who performed the action        |
| `action`       | String            | Action name (e.g., `document.create`)|
| `resource_type`| String            | Affected resource type               |
| `resource_id`  | UUID              | Affected resource identifier         |
| `version_id`   | UUID (nullable)   | Related version (if applicable)      |
| `request_id`   | String (nullable) | HTTP request tracing ID              |
| `ip`           | String (nullable) | Client IP address                    |
| `user_agent`   | String (nullable) | Client user agent                    |
| `outcome`      | String            | `success` or `failure`               |
| `details`      | JSONB             | Additional context (free-form JSON)  |

The `details` field carries action-specific data, such as the new status after
a transition or the retention date that was set.
