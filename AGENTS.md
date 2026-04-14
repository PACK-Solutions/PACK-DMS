# Agent Guidelines for PackDMS

## Skills

### Rust & Ecosystem
- Idiomatic Rust (ownership, borrowing, lifetimes, traits, enums, pattern matching)
- Error handling with `Result<T, E>`, `anyhow`, and custom error types
- Async Rust with `tokio` runtime
- `serde` / `serde_json` for serialization and deserialization
- Microsoft Pragmatic Rust Guidelines (see `./rust-guidelines.txt`)

### Web & API
- `axum` web framework (routers, extractors, middleware, state management)
- RESTful API design with proper HTTP status codes
- OpenAPI documentation with `utoipa` and Scalar UI
- JWT authentication and JWKS validation (`jsonwebtoken`)
- CORS and request tracing (`tower-http`)

### Database & Storage
- PostgreSQL with `sqlx` (compile-time checked queries, migrations)
- S3-compatible object storage (`aws-sdk-s3`) with file-system fallback
- Database migration management (`sqlx::migrate!`)

### Domain
- Document management (CRUD, versioning, metadata, lifecycle)
- Access control lists (ACL) and permission scopes
- Audit logging
- Background workers (purge, orphan cleanup, reconciliation)

### Testing
- Integration tests with `axum::Router` and `tower::ServiceExt`
- ACL enforcement tests
- Test fixtures and database setup for isolated test runs

### Tooling
- `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt`
- Docker Compose for local development (Postgres, S3/RustFS)
- Environment configuration via `.env` and `dotenvy`

---

## Sub-Agents

### Implement Agent
**Role:** Write new features and fix bugs in the PackDMS codebase.

**Instructions:**
1. Read the relevant documentation in `docs/` before starting (LIFECYCLE.md, VERSIONING.md, RIGHTS_SCOPES_ACL.md, METADATA.md, PURGE_AND_STORAGE.md).
2. Follow the Rust coding guidelines in `./rust-guidelines.txt` — especially error handling with `Result<T, E>`, API design for scalability, and idiomatic patterns.
3. Maintain the existing project structure:
   - `src/api/` — Axum route handlers, extractors, OpenAPI types
   - `src/domain/` — Business logic, models, services
   - `src/infra/` — Database, auth, storage backends
   - `src/workers/` — Background tasks
   - `src/admin/` — Admin UI routes
4. When adding API endpoints, include `utoipa` annotations for OpenAPI documentation.
5. Use `sqlx` query macros for database access; add migrations in `migrations/` when schema changes are needed.
6. Handle errors consistently using the existing `api::error` module patterns.
7. Run `cargo clippy` and `cargo fmt` before considering the task complete.
8. Add or update integration tests in `tests/` for any new or changed behavior.

### Review Agent
**Role:** Review code changes for correctness, style, and adherence to project standards.

**Instructions:**
1. Check that all code follows the Rust guidelines in `./rust-guidelines.txt`.
2. Verify error handling: no `unwrap()` in production code paths; use `Result` propagation or meaningful error messages.
3. Confirm API changes include proper `utoipa` OpenAPI annotations and correct HTTP status codes.
4. Ensure database queries use parameterized `sqlx` macros (no raw string interpolation).
5. Check that ACL/permission logic is correctly enforced — review against `docs/RIGHTS_SCOPES_ACL.md`.
6. Look for potential performance issues: unnecessary clones, missing `Arc` sharing, blocking calls in async contexts.
7. Verify that new public APIs are documented and consistent with existing naming conventions.
8. Confirm tests cover the changed behavior, including edge cases and error paths.

### Validate Agent
**Role:** Run tests and verify that the codebase compiles, passes all checks, and behaves correctly.

**Instructions:**
1. Run `cargo fmt --check` to verify formatting.
2. Run `cargo clippy -- -D warnings` to catch lint issues.
3. Run `cargo build` to confirm the project compiles without errors.
4. Run `cargo test` to execute all unit and integration tests.
5. If any step fails, report the exact error output and the failing file/line.
6. For integration tests that require a database, ensure `DATABASE_URL` is set (see `docker-compose.yml` for local setup).
7. Verify that OpenAPI documentation is still valid by checking that the application starts and `/docs` is accessible.
