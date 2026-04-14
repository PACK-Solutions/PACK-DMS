# Rights, Scopes & ACL — Authorization Model

This document describes the three-layer authorization model used by PackDMS.

---

## 1. Scopes (Coarse-Grained — JWT Claims)

Scopes are **coarse-grained** permissions carried inside the JWT `scope` claim.
They gate access at the **API endpoint level** — a request is rejected before any
business logic runs if the caller lacks the required scope.

| Scope            | Purpose                                      |
|------------------|----------------------------------------------|
| `document:read`  | Read document metadata, list, search, download versions |
| `document:write` | Create/update documents, upload versions, manage ACLs   |
| `admin`          | Administrative operations (legal hold, retention, etc.) |

Scopes are checked via `AuthContext::require_scope()` at the top of every handler.

---

## 2. Rights / Permissions (Fine-Grained — Per-Document)

Permissions are **fine-grained** grants that control what a principal can do on a
**specific document**. They are stored as `DocumentAcl` rows in the database.

| Permission | Implies        | Typical Use                        |
|------------|----------------|------------------------------------|
| `read`     | —              | View document metadata & versions  |
| `write`    | `read`         | Edit metadata, upload new versions |
| `admin`    | `write`, `read`| Manage ACL entries on the document |

Permission implication means that granting `admin` automatically grants `write`
and `read`; granting `write` automatically grants `read`.

### Owner Privilege

The **document owner** (`documents.owner_id`) always receives implicit `admin`
permission — no ACL entry is required.

---

## 3. Access Control Lists (ACLs)

An ACL is a set of `DocumentAcl` entries that map **principals** to
**permissions** on a specific document.

### Principal Types

| `principal_type` | Identifier Field | Description                     |
|------------------|------------------|---------------------------------|
| `user`           | `principal_id`   | A specific user (by UUID)       |
| `role`           | `role`           | All users holding a named role  |

### Effective Permission Resolution

Given a user ID, their roles, and a document ID, the `AclService` computes the
**effective permission set** as follows:

1. If the user is the document **owner** → return `{admin, write, read}`.
2. Query all `DocumentAcl` entries matching the user's ID or any of their roles.
3. Compute the **union** of implied permissions from all matching entries.
4. If no matching entries exist, **walk up the parent hierarchy** (`parent_id`)
   and repeat from step 2 on the parent document (up to 20 levels).
5. If still empty → the user has **no permissions** on the document.

### ACL Inheritance (Folder / Collection Hierarchy)

Documents may have an optional `parent_id` referencing another document (acting
as a folder or collection). When a document has **no explicit ACL entries**, the
system walks up the parent chain to find inherited permissions. The first
ancestor with matching ACL entries determines the effective permissions.

### API Endpoints

| Method  | Path                      | Description                          |
|---------|---------------------------|--------------------------------------|
| `GET`   | `/documents/{id}/acl`     | List ACL entries for a document      |
| `PUT`   | `/documents/{id}/acl`     | Replace all ACL entries (full reset) |
| `PATCH` | `/documents/{id}/acl`     | Add or remove individual entries     |

The `PATCH` endpoint accepts an array of operations:

```json
[
  { "op": "add",    "principal_type": "user", "principal_id": "...", "permission": "read" },
  { "op": "remove", "principal_type": "role", "role": "editors",    "permission": "write" }
]
```

### Enforcement

ACL enforcement is applied in every document and version handler via the
`enforce_permission` guard (`src/api/acl_guard.rs`). If the caller lacks the
required document-level permission, a **403 Forbidden** response is returned.

For list/search endpoints, results are **post-filtered** to include only
documents the caller has at least `read` permission on.

### Auto-Created ACL

When a document is created, a default ACL entry granting `admin` to the
document owner is automatically inserted. This ensures the owner can always
manage the document's permissions even if the ACL is later replaced.
