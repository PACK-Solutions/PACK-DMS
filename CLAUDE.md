# PackDMS — Document Management System API

Secure DMS API built in Rust (edition 2024).

## Tech Stack

- **Framework**: Axum 0.8 — **Database**: PostgreSQL 16 via SQLx — **Storage**: RustFS (S3-compatible) via `aws-sdk-s3`
- **Auth**: JWT/JWKS (`jsonwebtoken`) — **API docs**: OpenAPI via `utoipa` + Scalar UI — **Logging**: `tracing`
- **Infra**: Docker Compose (PostgreSQL + RustFS)

## Architecture

```
src/
├── api/       # HTTP handlers, DTOs, OpenAPI annotations, error mapping
├── domain/    # Business models and rules (no framework dependencies)
├── infra/     # Database (db/), auth, storage adapters
├── workers/   # Background tasks (purge, orphan cleanup, reconciliation)
├── admin/     # Admin UI routes
└── main.rs / lib.rs
```

- `migrations/` — SQLx migrations (`YYYYMMDDHHMMSS_description.sql`)
- `tests/` — Integration tests
- `examples/` — Utilities (e.g., `gen_jwks`)
- `docs/` — Domain documentation (LIFECYCLE, VERSIONING, RIGHTS_SCOPES_ACL, METADATA, PURGE_AND_STORAGE)

## Build & Run

```bash
docker-compose up -d                    # Start PostgreSQL + RustFS
cargo run --example gen_jwks            # Generate dev JWT keys
cargo build                             # Debug build
cargo run                               # Start server (localhost:8080, auto-migrations)
```

- Scalar UI: `http://localhost:8080/docs`
- Config via `.env` (see docker-compose.yml for defaults)

## Quality Commands

```bash
cargo fmt --check                       # Check formatting
cargo clippy -- -D warnings             # Lint (zero warnings policy)
cargo test                              # All tests (unit + integration)
cargo test --test integration           # Integration tests only
```

## Coding Conventions

- Follow Microsoft Pragmatic Rust Guidelines: @rust-guidelines.txt
- **Error handling**: `Result<T, E>` everywhere. API errors mapped via `api/error.rs`. No `.unwrap()` in production.
- **Async**: All I/O through `tokio`. Never block the runtime.
- **Database**: SQLx compile-time checked queries. All IDs are `Uuid`, timestamps are `DateTime<Utc>`.
- **API endpoints**: Require JWT Bearer auth (except `/docs`). Annotate all handlers with `utoipa` macros.
- **Traits**: Use `async-trait` for async trait definitions (e.g., storage backends).
- **Derives**: `serde::Serialize`/`Deserialize` for DTOs, `utoipa::ToSchema` for OpenAPI types.
- **Style**: `rustfmt` defaults, `clippy` clean, `snake_case` functions, `PascalCase` types.

## Integration Tests

- Require running PostgreSQL (`docker-compose up -d`) and `DATABASE_URL` set
- Use `axum::Router` + `tower::ServiceExt::oneshot` (no real server needed)
- Follow Arrange-Act-Assert pattern with `#[tokio::test]`
