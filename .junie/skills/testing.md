# Testing – PackDMS

## Running Tests

```bash
cargo test                        # all tests (unit + integration)
cargo test --lib                  # unit tests only
cargo test --test integration     # integration tests only
```

## Test Structure

- **Unit tests**: Inline `#[cfg(test)]` modules within `src/` files.
- **Integration tests**: `tests/integration.rs` – tests the API through HTTP using `axum::Router` and `tower::ServiceExt`.

## Integration Test Requirements

Integration tests require:
1. A running PostgreSQL instance (via `docker-compose up -d`)
2. Environment variables set (via `.env` or exported)
3. Optionally a running RustFS instance for storage tests

## Writing Tests

- Use `#[tokio::test]` for async tests.
- For integration tests, build the app router and use `tower::oneshot` to send requests without starting a real server.
- Use `sqlx::PgPool` for database setup/teardown in tests.
- Follow the Arrange-Act-Assert pattern.

## Dev Dependencies

- `tower` – for calling the Axum router directly in tests
