# Project Overview – PackDMS

PackDMS is a secure Document Management System (DMS) API built with Rust.

## Tech Stack

- **Language**: Rust (edition 2024)
- **Web framework**: Axum 0.8
- **Database**: PostgreSQL 16 via SQLx (async, compile-time checked queries)
- **Object storage**: RustFS (S3-compatible) via `aws-sdk-s3`
- **Authentication**: JWT with JWKS (`jsonwebtoken` crate)
- **API docs**: OpenAPI via `utoipa` + Scalar UI
- **Logging**: `tracing` + `tracing-subscriber`
- **Infrastructure**: Docker Compose (PostgreSQL + RustFS)

## Module Structure

```
src/
├── main.rs          # Entry point, server bootstrap
├── lib.rs           # Public module declarations
├── api/             # HTTP handlers, request/response types, error mapping
│   ├── documents.rs
│   ├── versions.rs
│   ├── acl.rs
│   ├── audit.rs
│   ├── types.rs
│   └── error.rs
├── domain/          # Business models and logic
│   └── models.rs
├── infra/           # Infrastructure adapters
│   ├── db/          # PostgreSQL repository layer
│   ├── auth/        # JWT/JWKS authentication
│   └── storage/     # S3/local file storage abstraction
└── workers/         # Background workers
```

## Key Directories

- `migrations/` – SQLx SQL migration files (timestamp-prefixed)
- `tests/` – Integration tests (`integration.rs`)
- `examples/` – Utility binaries (e.g., `gen_jwks` for local key generation)
- `api-requests/` – Sample HTTP request files
- `data/` – Local data/keys directory
