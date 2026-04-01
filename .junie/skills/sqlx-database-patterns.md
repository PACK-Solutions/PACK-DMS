# SQLx & PostgreSQL Patterns

## Connection Management

- Use `PgPoolOptions::new().max_connections(n).connect(&url).await` to create a connection pool.
- Pass `PgPool` (or `Arc<PgPool>`) via Axum `State` — pools are already `Clone` and internally `Arc`-wrapped.
- Set `max_connections` based on expected concurrency (default 10; tune per workload).
- Use `.connect_lazy(&url)` for deferred connection (useful in tests or CLI tools).
- Always handle connection errors gracefully — the pool retries internally but can still fail.

## Query Patterns

### Compile-time checked queries (preferred)
```rust
// Returns anonymous struct with typed fields
let row = sqlx::query!("SELECT id, name FROM users WHERE id = $1", user_id)
    .fetch_one(&pool)
    .await?;

// Map to a named struct
let user = sqlx::query_as!(User, "SELECT id, name, email FROM users WHERE id = $1", user_id)
    .fetch_one(&pool)
    .await?;
```
- Requires `DATABASE_URL` at compile time (set in `.env` or environment).
- Validates SQL syntax, column types, and parameter types at compile time.
- Use `sqlx::query_scalar!` for single-value queries (e.g., `SELECT count(*) ...`).

### Runtime queries
```rust
let rows = sqlx::query("SELECT * FROM users WHERE active = $1")
    .bind(true)
    .fetch_all(&pool)
    .await?;
```
- Use when SQL is dynamic or compile-time checking is impractical.

## Fetch Methods

- `fetch_one(&pool)` – exactly one row; errors if 0 or 2+ rows.
- `fetch_optional(&pool)` – returns `Option<Row>`; use for lookups that may miss.
- `fetch_all(&pool)` – returns `Vec<Row>`; loads all into memory.
- `fetch(&pool)` – returns a `Stream` of rows; use for large result sets to avoid memory pressure.

## Transactions

```rust
let mut tx = pool.begin().await?;
sqlx::query!("INSERT INTO users (name) VALUES ($1)", name)
    .execute(&mut *tx)
    .await?;
sqlx::query!("INSERT INTO audit_log (action) VALUES ($1)", "user_created")
    .execute(&mut *tx)
    .await?;
tx.commit().await?;
// If tx is dropped without commit, it automatically rolls back.
```
- Always use transactions for multi-statement operations.
- Pass `&mut *tx` (deref) to query execution within transactions.
- Transactions auto-rollback on drop — no explicit rollback needed for error paths.

## Migrations

- Store in `migrations/` directory with format `YYYYMMDDHHMMSS_description.sql`.
- Run with `sqlx::migrate!().run(&pool).await?` at startup.
- Use `sqlx migrate add <name>` CLI to create new migration files.
- Migrations are idempotent — already-applied migrations are skipped.
- Use `IF NOT EXISTS` / `IF EXISTS` in DDL for safety.

## Type Mapping

| Rust Type | PostgreSQL Type |
|-----------|----------------|
| `i32` | `INT4` |
| `i64` | `INT8` / `BIGINT` |
| `f64` | `FLOAT8` / `DOUBLE PRECISION` |
| `String` | `TEXT` / `VARCHAR` |
| `bool` | `BOOL` |
| `Uuid` | `UUID` (with `uuid` feature) |
| `DateTime<Utc>` | `TIMESTAMPTZ` (with `chrono` feature) |
| `NaiveDate` | `DATE` (with `chrono` feature) |
| `serde_json::Value` | `JSONB` (with `json` feature) |
| `Vec<u8>` | `BYTEA` |
| `Option<T>` | nullable column |

## Performance Tips

- Use `RETURNING` clauses to avoid extra SELECT after INSERT/UPDATE.
- Prefer `fetch_optional` over `fetch_one` + error handling for lookups.
- Use `$1 = ANY($2)` with `&[Uuid]` for IN-clause queries instead of string interpolation.
- Index columns used in WHERE, JOIN, and ORDER BY clauses.
- Use `EXPLAIN ANALYZE` to verify query plans during development.
- For bulk inserts, use `UNNEST` or build multi-row VALUES with `QueryBuilder`.

## Testing with SQLx

- Use a dedicated test database; run migrations before tests.
- Wrap each test in a transaction and roll back for isolation:
  ```rust
  let mut tx = pool.begin().await.unwrap();
  // ... test operations on &mut *tx ...
  tx.rollback().await.unwrap();
  ```
- Alternatively, use `#[sqlx::test]` macro for automatic test database setup.
