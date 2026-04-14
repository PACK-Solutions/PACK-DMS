# PackDMS API

PackDMS is a secure Document Management System (DMS) API built with Rust. It supports document versioning, metadata search, access control lists (ACL), and audit logging.

## Features

- **Document Management**: Create, update, and retrieve documents.
- **Versioning**: Upload and download multiple versions of a document.
- **Security**: JWT-based authentication with JWKS support.
- **ACL**: Manage permissions at the document level.
- **Search**: Search documents by metadata and other criteria.
- **Audit Logging**: Track system-wide actions for compliance.
- **Documentation**: Built-in OpenAPI documentation with Scalar.
- **Admin Frontend**: Built-in administration UI for managing documents, versions, ACLs, and audit logs.
- **S3-compatible Storage**: Binary content stored in RustFS (or any S3-compatible backend) with support for Object Lock / WORM, retention, versioning, encryption, and replication.

## Prerequisites

- **Rust**: Latest stable version.
- **Docker**: For running PostgreSQL and RustFS.

## Getting Started

### 1. Start the Infrastructure

Start PostgreSQL and RustFS using Docker Compose:

```bash
docker-compose up -d
```

This starts:
- **PostgreSQL** on port `5432` – stores all business metadata.
- **RustFS** on port `9000` (S3 API) and `9001` (web console) – stores binary document content.

The RustFS web console is available at [http://localhost:9001](http://localhost:9001) (default credentials: `minioadmin` / `minioadmin`).

### 2. Configure the Environment

Create a `.env` file in the root directory:

```env
DATABASE_URL=postgres://postgres:password@localhost:5432/packdms
JWT_ISSUER=https://example.com/auth
JWKS_URL=data/keys/jwks.json
BIND=0.0.0.0:8080
RUST_LOG=info,tower_http=info

# S3-compatible storage (RustFS)
S3_ENDPOINT_URL=http://localhost:9000
S3_BUCKET=packdms
S3_REGION=us-east-1
AWS_ACCESS_KEY_ID=minioadmin
AWS_SECRET_ACCESS_KEY=minioadmin
```

> **Note:** If `S3_ENDPOINT_URL` is not set, PackDMS falls back to local file-system storage using `STORAGE_PATH` (default `./data`).

### 3. Generate Local Keys (for Development)

If you don't have an external OIDC provider, you can generate a local RSA key pair and JWKS file for development purposes:

```bash
cargo run --example gen_jwks
```

This will create `data/keys/private.pem` and `data/keys/jwks.json`.

### 4. Run Migrations

Database migrations are automatically run when the application starts. Ensure the `DATABASE_URL` is correctly set.

### 5. Compile and Run the API

```bash
cargo run
```

The API will be available at `http://localhost:8080`.

## API Documentation

Once the server is running, you can access the interactive API documentation at:

- **Scalar**: [http://localhost:8080/docs](http://localhost:8080/docs)

## Administration Frontend

PackDMS includes a built-in administration UI available at:

- **Admin**: [http://localhost:8080/admin](http://localhost:8080/admin)

The admin frontend provides a complete interface for all API operations:

- Create, search, and manage documents (metadata, status transitions, legal hold, retention).
- Upload, download, preview, and delete document versions.
- Configure per-document access control lists (ACL).
- View system-wide audit logs.
- Preview documents in known formats (PDF, images, text, video, audio) directly in the browser.

It is built with HTMX and Tailwind CSS — no JavaScript build step required. The HTML is embedded at compile time via `include_str!`.

## API Endpoints

All endpoints (except documentation) require a valid JWT Bearer token in the `Authorization` header.

### Documents

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/documents` | Create a new document. |
| `GET` | `/documents` | Search documents. |
| `GET` | `/documents/{id}` | Get document details by ID. |
| `PATCH` | `/documents/{id}` | Update document metadata or title. |
| `POST` | `/documents/{id}/status` | Change document status (including soft-delete). |
| `POST` | `/documents/{id}/restore` | Restore a soft-deleted document. |
| `POST` | `/documents/{id}/legal-hold` | Set or clear legal hold. |
| `POST` | `/documents/{id}/retention` | Set or clear retention policy. |

### Versions

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/documents/{id}/versions` | Upload a new version of a document (multipart/form-data). |
| `GET` | `/documents/{id}/versions` | List all versions of a document. |
| `GET` | `/documents/{id}/versions/{vid}/download` | Download a specific version of a document. |
| `DELETE` | `/documents/{id}/versions/{vid}` | Soft-delete a specific version. |

### ACL & Audit

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/documents/{id}/acl` | Get Access Control List for a document. |
| `PUT` | `/documents/{id}/acl` | Update Access Control List for a document. |
| `GET` | `/audit` | List system-wide audit logs (Admin only). |

## Document Storage Architecture

PackDMS uses a **dual storage strategy**:

- **PostgreSQL** stores all business metadata: document records (title, status, owner, timestamps, custom metadata JSON), version records (version number, content hash, size, MIME type, storage key), ACLs, and audit logs.
- **RustFS (S3-compatible)** stores the binary file content of each document version. RustFS object keys are used only as technical identifiers — all business metadata lives in PostgreSQL.

### S3 Object Key Convention

Object keys follow a hierarchical, tenant-aware pattern:

```
tenant/{ownerId}/document/{documentId}/v{version}/original/{filename}
```

Buckets can be organized by environment or data sensitivity (e.g., `packdms-dev`, `packdms-prod`, `packdms-confidential`).

### What RustFS Handles

By delegating binary storage to an S3-compatible backend, PackDMS benefits from:

- **Object Lock / WORM** – immutable retention for compliance.
- **Document retention policies** – automatic lifecycle management.
- **Versioning** – S3-level object versioning as an additional safety net.
- **Server-side encryption** – data encrypted at rest.
- **Replication** – cross-site or cross-region redundancy.
- **Erasure coding** – data durability beyond simple replication.

### Document Lifecycle: Status, Soft-Delete, Purge & Locks

Every document goes through a series of statuses that reflect where it is in its lifecycle:

| Status | Meaning |
|--------|---------|
| **Draft** | The document has been created but is not yet published. |
| **Active** | The document is published and in use. |
| **Archived** | The document is preserved for long-term storage. |
| **Deleted** | The document has been soft-deleted and is awaiting purge or restore. |
| **Purged** | The document has been permanently removed (terminal — cannot be restored). |

#### Status Transitions

Documents move through their lifecycle via `POST /documents/{id}/status`:

```
Draft  ──▶  Active  ◀──▶  Archived
  │            │               │
  └────────────┴───────────────┘
               │
               ▼
           Deleted  ──restore──▶  Draft
               │
               ▼  (automatic)
            Purged
```

- **Draft → Active** — publish the document for use.
- **Active → Archived** — move the document to long-term storage.
- **Archived → Active** — reactivate an archived document.
- **Draft / Active / Archived → Deleted** — soft-delete the document (blocked if under legal hold or active retention).
- **Deleted → Draft** — restore a soft-deleted document via `POST /documents/{id}/restore`.

#### Soft-Delete

Soft-deleting a document does **not** remove any data. The document and all its versions remain in the database and storage. A soft-deleted document can be restored at any time — as long as it has not yet been purged.

#### Automatic Purge

A background worker runs periodically and permanently purges documents that meet **either** of these conditions (provided the document is **not** under legal hold):

1. **Soft-deleted documents** whose retention date has expired or was never set.
2. **Any document** (even if still active, draft, or archived) whose retention date has passed.

When a document is purged:
- All associated versions are removed.
- The document reaches the **purged** status, which is terminal — it cannot be restored.
- An audit log entry is recorded for traceability.

#### Legal Hold

Setting legal hold on a document (via `POST /documents/{id}/legal-hold`) acts as an **absolute protection**: the document **cannot** be soft-deleted by users and is **excluded from automatic purge** — regardless of its retention date or current status. Legal hold must be explicitly lifted before the document can be deleted or purged. Only administrators can manage legal hold.

#### Retention Policy

A retention date (set via `POST /documents/{id}/retention`) defines the earliest point at which a document may be deleted or purged. While the retention period is active, soft-deletion is blocked.

**Important:** once the retention date passes, the document becomes eligible for automatic purge — even if it was never soft-deleted. To prevent this, either clear the retention date before it expires or place the document under legal hold. Only administrators can manage retention policies.

### Legal Hold, Retention & Versioning Strategy

PackDMS manages **legal hold**, **retention locks**, and **document versioning** at the
**PostgreSQL application level**, not via S3 Object Lock or S3 bucket versioning.

**Why this choice:**

- **Portability** — Works with any S3-compatible backend (RustFS, MinIO, AWS S3)
  regardless of Object Lock support. RustFS does not reliably support
  `ObjectLockConfiguration`.
- **Consistency** — Legal hold, retention, and version state changes happen inside
  PostgreSQL transactions alongside status updates and audit logs, guaranteeing
  atomicity.
- **Queryability** — All lifecycle state is queryable via SQL: "which documents are
  under legal hold?", "which retention periods expire this month?", etc.
- **Simplicity** — No need to configure S3 buckets with `--object-lock-enabled`
  (which must be set at bucket creation and cannot be added later), and no risk of
  irrevocable Compliance-mode locks from operational mistakes.

**Trade-off:** If someone has direct access to the S3 bucket (compromised
credentials, rogue admin), they could delete objects bypassing the application-level
legal hold. For environments requiring regulatory-grade immutability (SEC 17a-4,
WORM), S3 Object Lock can be added as a **defense-in-depth layer** on top of the
PostgreSQL controls — the existing `job_outbox` pattern supports best-effort
synchronization of S3 locks without changing the core logic.

RustFS features like server-side encryption, erasure coding, and replication still
apply to all stored objects regardless of this choice.

### Save Flow (version upload)

1. The uploaded file is received via multipart form-data.
2. A SHA-256 content hash is computed and a hierarchical storage key is generated.
3. The binary content is written to RustFS via the S3 `PutObject` API.
4. Within a single database transaction:
   - A `document_versions` row is inserted (referencing the S3 object key).
   - The parent `documents` row is updated with the new `current_version_id`.
   - An audit log entry is recorded.
5. The transaction is committed.

This separation keeps PostgreSQL lean (metadata and indexes only) while large binary payloads benefit from S3-grade durability, encryption, and lifecycle management.

## Development

### Running Tests

```bash
cargo test
```

Tests use an in-memory blob store (`MemoryBlobStore`) and do not require RustFS.

### Testing the API in Development Mode

Follow these steps to get a fully working local environment with JWT authentication:

#### Step 1 – Start Infrastructure & Generate Keys

```bash
docker-compose up -d          # PostgreSQL + RustFS
cargo run --example gen_jwks  # RSA key pair + JWKS file → data/keys/
```

#### Step 2 – Generate Development JWT Tokens

```bash
cargo run --example gen_jwt
```

This generates two tokens (valid for 1 hour):

| Token | Subject (sub) | Scopes |
|-------|---------------|--------|
| **User** | `00000000-…-000001` / `user@example.com` | `read write` |
| **Admin** | `00000000-…-000002` / `admin@example.com` | `read write admin` |

The command also auto-patches `api-requests/http-client.env.json` with the fresh tokens (if the placeholders are still present).

> **Tip:** Tokens expire after 1 hour. Re-run `cargo run --example gen_jwt` to get new ones. Reset the placeholders in `http-client.env.json` if you want auto-patching again.

#### Step 3 – Start the API

```bash
cargo run
```

#### Step 4 – Call the API

**Option A – IntelliJ / RustRover HTTP Client (recommended)**

Open `api-requests/documents.http` in the IDE. Select the **dev** environment — the `auth_token` and `admin_token` variables are populated by the previous step. Run requests sequentially to walk through the full document lifecycle.

**Option B – curl**

```bash
# Paste the user token from gen_jwt output
TOKEN="eyJhbGciOi…"

# Create a document
curl -s http://localhost:8080/documents \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"title":"Test Doc","metadata":{"type":"note"}}'

# Search documents
curl -s http://localhost:8080/documents?limit=10 \
  -H "Authorization: Bearer $TOKEN"
```

### DB / Storage Reconciliation

PackDMS runs three background workers (spawned as Tokio tasks at startup) to keep PostgreSQL and RustFS in sync:

| Worker | Interval | Purpose |
|--------|----------|---------|
| **Blob purge** | 30 s | Deletes objects from RustFS for blobs marked `pending_deletion` with `ref_count = 0`. |
| **Document purge** | 60 s | Purges documents whose retention period has expired or that are soft-deleted past retention (see below). |
| **Orphan cleanup** | 5 min | Processes `cleanup_orphans` jobs from the `job_outbox` table. |
| **Reconciliation** | 1 h | Verifies that active blobs recorded in the database actually exist in storage. |

#### Why batch-of-active-blobs instead of full bucket listing?

A naïve reconciliation would call `ListObjectsV2` on the entire bucket and compare every key against the database. This approach has several drawbacks on S3-compatible stores (RustFS / MinIO):

1. **Cost** — `ListObjectsV2` is billed per request (1 000 keys per page). A bucket with millions of objects generates thousands of API calls per reconciliation run.
2. **Latency** — Listing is paginated and sequential; a large bucket can take minutes to enumerate, during which the worker holds resources.
3. **Consistency window** — S3 listing is eventually consistent for recently written objects, so a full listing may report false positives (missing keys that were just uploaded).

Instead, the reconciliation worker queries the `blobs` table for a **batch of 100 active blobs** (ordered by `created_at ASC`) and issues a lightweight `HeadObject` call for each one:

- **Object present** → size is compared with the value stored in the database; a mismatch is logged as a warning.
- **Object missing** → an error is logged indicating a DB/storage inconsistency.
- **Head call fails** → a warning is logged and the blob is retried on the next cycle.

This design keeps reconciliation **O(batch_size)** per cycle rather than **O(bucket_size)**, making it safe to run frequently (every hour by default) even on large deployments.

> **Limitation:** This approach does not detect *storage orphans* — objects present in RustFS but absent from the database. Detecting those would require a full bucket listing. A dedicated admin job or CLI command can be added later for periodic deep reconciliation if needed.

### Code Structure

- `src/api`: API route handlers and request/response models.
- `src/domain`: Core domain models and business logic.
- `src/infra`: Infrastructure layer (database, storage, auth).
- `src/infra/storage`: `BlobStore` trait with `S3BlobStore`, `FileBlobStore`, and `MemoryBlobStore` implementations.
- `migrations`: SQL migrations for the database.
- `examples`: Utility scripts (JWKS generation, JWT token generation).
