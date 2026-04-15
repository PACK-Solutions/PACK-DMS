# Document Lifecycle

This document describes how documents move through their lifecycle in PackDMS,
including status transitions, protection mechanisms, and restoration.

---

## 1. Lifecycle States

Every document has a `status` field that tracks its current lifecycle stage.

| Status     | Description                                              |
|------------|----------------------------------------------------------|
| `draft`    | Initial state after creation. Content can be uploaded.   |
| `active`   | Published document, available for normal use.            |
| `archived` | Preserved for long-term storage; can be reactivated.     |
| `deleted`  | Soft-deleted; data is retained but hidden from listings. |

---

## 2. Allowed Transitions

```
         ┌───────────┐
         │  draft    │
         └────┬──┬───┘
              │  │
    activate  │  │  delete
              ▼  ▼
         ┌────────┐        ┌─────────┐
         │ active │◄──────►│archived │
         └───┬────┘restore └────┬────┘
             │    archive       │
       delete│            delete│
             ▼                  ▼
         ┌────────────────────────┐
         │        deleted         │
         └────────────────────────┘
```

| From       | To         | Trigger                                |
|------------|------------|----------------------------------------|
| `draft`    | `active`   | `POST /documents/{id}/status` with `{"status":"active"}`   |
| `active`   | `archived` | `POST /documents/{id}/status` with `{"status":"archived"}` |
| `archived` | `active`   | `POST /documents/{id}/status` with `{"status":"active"}`   |
| `draft`    | `deleted`  | `POST /documents/{id}/status` with `{"status":"deleted"}`  |
| `active`   | `deleted`  | `POST /documents/{id}/status` with `{"status":"deleted"}`  |
| `archived` | `deleted`  | `POST /documents/{id}/status` with `{"status":"deleted"}`  |

Invalid transitions (e.g., `deleted → active`) return **400 Bad Request**.

---

## 3. Soft-Delete vs. Permanent Purge

PackDMS uses a **two-phase deletion** model:

1. **Soft-delete** — The document's status is set to `deleted`, and `deleted_at`
   / `deleted_by` are recorded. No data is removed; the document and all its
   versions remain in the database and blob storage.

2. **Permanent purge** — A background worker eventually removes the document,
   its versions, and the underlying blobs from both the database and storage.
   Once purged, the document **cannot** be restored.

### What Happens on Soft-Delete

- `status` → `deleted`
- `deleted_at` → current timestamp
- `deleted_by` → ID of the user who performed the deletion
- An audit log entry (`document.status`) is created
- The document is excluded from `list_documents` and `search_documents` results

---

## 4. Restoration

Soft-deleted and archived documents can be restored via
`POST /documents/{id}/restore`.

| Original Status | Restored To | Fields Cleared              |
|-----------------|-------------|-----------------------------|
| `deleted`       | `draft`     | `deleted_at`, `deleted_by`  |
| `archived`      | `active`    | `archived_at`               |

Documents that have been **permanently purged** cannot be restored.

---

## 5. Protection Mechanisms

Two mechanisms prevent deletion and purge of important documents:

### Legal Hold

- Set via `POST /documents/{id}/legal-hold` (requires `admin` scope)
- When `legal_hold = true`, the document **cannot** be soft-deleted by users
  and is **excluded** from automatic purge by background workers
- Must be explicitly lifted before the document can be deleted

### Retention Period

- Set via `POST /documents/{id}/retention` (requires `admin` scope)
- Defines a `retention_until` date — the earliest point at which the document
  may be deleted or purged
- While the retention period is active (`retention_until > now`), soft-deletion
  is blocked
- After the retention date passes, the document becomes eligible for automatic
  purge by the background worker

### Combined Protection

A document is considered **protected** if either condition is true:

```
is_protected = legal_hold OR (retention_until > now)
```

Protected documents cannot be soft-deleted (returns **400 Bad Request**) and
are skipped by the purge worker.

---

## 6. Archival

Archiving moves a document to long-term storage status:

- `status` → `archived`
- `archived_at` → current timestamp
- The document remains accessible (read/download) but signals that it is no
  longer actively managed
- Can be reactivated via `POST /documents/{id}/status` with `active`, or
  restored via `POST /documents/{id}/restore`

---

## 7. Version Lifecycle

Each document version has its own lifecycle status:

| Status       | Description                                          |
|--------------|------------------------------------------------------|
| `active`     | Current or historical version, available for download |
| `superseded` | Replaced by a newer version (still downloadable)     |
| `deleted`    | Soft-deleted; excluded from listings and downloads   |

When a new version is uploaded, the previous `active` version is marked as
`superseded`. Deleted versions cannot be downloaded (returns **404**).

Version deletion is also blocked if the parent document is protected (legal
hold or active retention).

---

## 8. Audit Trail

Every lifecycle transition is recorded in the `audit_log` table:

| Action                     | Trigger                          |
|----------------------------|----------------------------------|
| `document.create`          | New document created             |
| `document.status`          | Status change (including delete) |
| `document.restore`         | Document restored                |
| `document.update`          | Metadata updated                 |
| `document.legal_hold`      | Legal hold toggled               |
| `document.retention`       | Retention period changed         |
| `version.upload`           | New version uploaded             |
| `version.delete`           | Version soft-deleted             |
| `document.retention_purge` | Background purge after retention |
