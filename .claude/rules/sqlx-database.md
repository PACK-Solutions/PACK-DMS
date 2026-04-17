---
paths:
  - "src/infra/db/**/*.rs"
  - "migrations/**/*.sql"
---

# SQLx & PostgreSQL Patterns

## Queries
- **Prefer compile-time checked queries**: `sqlx::query!()` / `sqlx::query_as!()` (requires `DATABASE_URL` at compile time).
- Use `fetch_one` (exactly 1 row), `fetch_optional` (0 or 1), `fetch_all` (all in memory), `fetch` (stream).
- Use `RETURNING` clauses to avoid extra SELECTs after INSERT/UPDATE.
- Use `$1 = ANY($2)` with `&[Uuid]` for IN-clause queries.

## Transactions
```rust
let mut tx = pool.begin().await?;
sqlx::query!("...").execute(&mut *tx).await?;
tx.commit().await?;
// Auto-rollback on drop if not committed.
```

## Migrations
- File format: `YYYYMMDDHHMMSS_description.sql` in `migrations/`.
- Run at startup: `sqlx::migrate!().run(&pool).await?`.
- Use `IF NOT EXISTS` / `IF EXISTS` in DDL.

## Type Mapping
- `Uuid` ↔ `UUID`, `DateTime<Utc>` ↔ `TIMESTAMPTZ`, `serde_json::Value` ↔ `JSONB`
- `Option<T>` ↔ nullable column, `Vec<u8>` ↔ `BYTEA`
