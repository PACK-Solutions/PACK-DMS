# Coding Conventions – PackDMS

## Guidelines Reference

Follow the Microsoft Pragmatic Rust Guidelines documented in `rust-guidelines.txt` at the project root.

## Key Principles

- **Error handling**: Use `Result<T, E>` throughout. The API layer maps domain errors to HTTP responses via `api/error.rs`. Use `anyhow` for application-level errors and custom error types for domain logic.
- **Async everywhere**: All I/O operations are async using `tokio`. Database calls use `sqlx`, HTTP uses `axum`, storage uses `aws-sdk-s3`.
- **Layered architecture**:
  - `api/` – HTTP handlers, request/response DTOs, OpenAPI annotations
  - `domain/` – Business models and rules (no framework dependencies)
  - `infra/` – Database, auth, and storage adapters
  - `workers/` – Background processing
- **Traits for abstraction**: Use `async-trait` for async trait definitions (e.g., storage backends).
- **Derive macros**: Use `serde::Serialize`/`Deserialize` for all DTOs, `utoipa::ToSchema` for OpenAPI types.

## Code Style

- Use `rustfmt` defaults for formatting.
- Use `clippy` for linting: `cargo clippy -- -D warnings`.
- Prefer `snake_case` for functions/variables, `PascalCase` for types/traits.
- Keep modules focused and small; use `mod.rs` for module declarations.

## Database

- Migrations live in `migrations/` with timestamp prefix format: `YYYYMMDDHHMMSS_description.sql`.
- Use SQLx query macros for compile-time checked SQL when possible.
- All IDs are UUIDs (`uuid::Uuid`).
- Timestamps use `chrono::DateTime<Utc>`.

## API Design

- All endpoints require JWT Bearer authentication (except `/docs`).
- Use proper HTTP methods and status codes.
- Annotate all handlers with `utoipa` macros for OpenAPI generation.
- Use `axum::extract` for path params, query params, and JSON bodies.
