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

## Prerequisites

- **Rust**: Latest stable version.
- **Docker**: For running the PostgreSQL database.
- **PostgreSQL**: (Optional) If not using Docker.

## Getting Started

### 1. Start the Database

The easiest way to start the required PostgreSQL database is using Docker Compose:

```bash
docker-compose up -d
```

### 2. Configure the Environment

Create a `.env` file in the root directory (you can copy the provided example if available, or use the values below):

```env
DATABASE_URL=postgres://postgres:password@localhost:5432/packdms
JWT_ISSUER=https://example.com/auth
JWKS_URL=data/keys/jwks.json
STORAGE_PATH=./data
BIND=0.0.0.0:8080
RUST_LOG=info,tower_http=info
```

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

## API Endpoints

All endpoints (except documentation) require a valid JWT Bearer token in the `Authorization` header.

### Documents

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/documents` | Create a new document. |
| `GET` | `/documents` | Search documents. |
| `GET` | `/documents/{id}` | Get document details by ID. |
| `PATCH` | `/documents/{id}` | Update document metadata or title. |
| `POST` | `/documents/{id}/status` | Change document status. |

### Versions

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/documents/{id}/versions` | Upload a new version of a document (multipart/form-data). |
| `GET` | `/documents/{id}/versions` | List all versions of a document. |
| `GET` | `/documents/{id}/versions/{vid}/download` | Download a specific version of a document. |

### ACL & Audit

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/documents/{id}/acl` | Get Access Control List for a document. |
| `PUT` | `/documents/{id}/acl` | Update Access Control List for a document. |
| `GET` | `/audit` | List system-wide audit logs (Admin only). |

## Development

### Running Tests

```bash
cargo test
```

### Code Structure

- `src/api`: API route handlers and request/response models.
- `src/domain`: Core domain models and business logic.
- `src/infra`: Infrastructure layer (database, storage, auth).
- `migrations`: SQL migrations for the database.
- `examples`: Utility scripts (e.g., JWKS generation).
