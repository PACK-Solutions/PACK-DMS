---
name: sqlx-migration
description: Create a new SQLx migration file for PackDMS with the correct YYYYMMDDHHMMSS timestamp prefix, idempotent DDL, and downgrade safety. Use whenever the user asks to "add a migration", "change the schema", "add a column/table/index", "alter a column", "add a foreign key", or describes a database change that needs to be persisted. Also trigger when discussing schema evolution, even if the user hasn't said the word "migration".
---

# Create a SQLx migration

PackDMS uses SQLx's filesystem migrator with the **timestamped** filename convention:

```
migrations/YYYYMMDDHHMMSS_short_description.sql
```

Migrations run automatically at server startup via `sqlx::migrate!().run(&pool)` and once at the top of every integration test. There is no Down migration file — SQLx applies forward only — so every migration must be safe to re-apply against an already-migrated database (idempotent DDL).

## Generate the filename

Use the bundled script to produce the timestamp and full path. It guarantees a unique, monotonically increasing prefix and avoids hand-typing the date:

```bash
./.claude/skills/sqlx-migration/scripts/new_migration.sh "add_widget_tier_column"
```

The script prints the path it created. The description part should be short, snake_case, and describe the *change* (`add_widget_tier_column`, `index_documents_owner`), not the *reason* (`fix_bug`, `customer_request`).

If the script isn't usable (e.g., no shell available), construct the filename manually using UTC: `date -u +%Y%m%d%H%M%S` followed by `_<description>.sql`. Always check the existing `migrations/` directory and pick a timestamp strictly greater than the most recent file's prefix.

## Write idempotent DDL

Every statement must be safe to run a second time. The reason: developers re-run the test suite repeatedly against the same database, and Postgres errors on duplicate objects unless the DDL guards itself.

| Operation | Idempotent form |
|---|---|
| Create table | `CREATE TABLE IF NOT EXISTS ...` |
| Drop table | `DROP TABLE IF EXISTS ...` |
| Add column | `ALTER TABLE x ADD COLUMN IF NOT EXISTS y ...` |
| Drop column | `ALTER TABLE x DROP COLUMN IF EXISTS y` |
| Create index | `CREATE INDEX IF NOT EXISTS ix_... ON ...` |
| Create constraint | Wrap in a `DO $$ BEGIN ... EXCEPTION WHEN duplicate_object THEN NULL; END $$;` block (Postgres has no `IF NOT EXISTS` for constraints) |

For complex changes, use a `DO $$ ... $$;` block to inspect the catalog before acting:

```sql
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'widgets' AND column_name = 'tier'
    ) THEN
        ALTER TABLE widgets ADD COLUMN tier TEXT NOT NULL DEFAULT 'free';
    END IF;
END $$;
```

## Type conventions

PackDMS maps Rust types to Postgres consistently. Match the existing schema's choices to keep `query_as!` working:

| Rust | Postgres |
|---|---|
| `Uuid` | `UUID` |
| `DateTime<Utc>` | `TIMESTAMPTZ` |
| `serde_json::Value` | `JSONB` |
| `Option<T>` | column without `NOT NULL` |
| `Vec<u8>` | `BYTEA` |
| `String` | `TEXT` (not `VARCHAR(n)` — length is enforced in Rust) |

Primary keys are `UUID PRIMARY KEY` — generate them in Rust with `Uuid::new_v4()`, never with `gen_random_uuid()` in SQL. The reason: tests need to know the ID before insert.

Timestamps default to `TIMESTAMPTZ NOT NULL DEFAULT NOW()` for `created_at`. `updated_at` is set explicitly by the application code, not by triggers.

## NOT NULL columns on existing tables

Adding a `NOT NULL` column to a populated table without a default fails. The pattern is:

1. Add the column nullable (or with a sentinel default).
2. Backfill in the same migration with `UPDATE widgets SET tier = 'free' WHERE tier IS NULL;`.
3. `ALTER TABLE widgets ALTER COLUMN tier SET NOT NULL;` once the data is consistent.

If the table is small (< a few thousand rows in production) this is fine inline. For large tables, do the backfill in a worker and add the constraint in a follow-up migration.

## Indexes

Index names follow `ix_<table>_<columns>` (`ix_documents_owner_id`, `ix_audit_logs_target_id_created_at`). Use `CREATE INDEX IF NOT EXISTS` and consider `CREATE INDEX CONCURRENTLY` for production-sized tables — but note that `CONCURRENTLY` cannot run inside a transaction, so put it in its own migration file with no other DDL.

## Foreign keys

Match the parent column type exactly. Use `ON DELETE CASCADE` only when the dependent rows have no meaning without the parent (the `document_versions → documents` relationship in this codebase). Otherwise use `ON DELETE RESTRICT` (the default) and let the application enforce ordering.

## After writing the migration

- The file is automatically picked up — no registration needed; `sqlx::migrate!()` reads the directory at compile time.
- `cargo build` — SQLx verifies the migration file syntax and refreshes the offline query cache. **`sqlx::query!` macros validate against the live database**, so a running Postgres is required.
- `cargo test` — every integration test runs the migrator at startup, so a broken migration fails the whole suite immediately.
- If you added or changed columns referenced by `query_as!` or `query!` macros, you may need to rebuild with the database running so SQLx can re-check the queries.

## Common pitfalls in this codebase

- **Putting the migration before existing ones in time order.** SQLx's migrator tracks applied versions; a "new" migration with an older timestamp will be skipped on already-migrated databases. Always pick a timestamp greater than every existing file.
- **Forgetting `IF NOT EXISTS`.** The integration tests reuse a single database; non-idempotent DDL breaks the second run.
- **Mixing schema and data migrations.** A schema change plus a backfill is fine in one file. A pure data correction (e.g., fixing rows) belongs in a script under `examples/` or a one-off SQL invocation, not a migration.
- **Editing an applied migration.** Once a migration is committed and has run anywhere, it is immutable. Fix mistakes in a follow-up migration.

## Related project docs

- `.claude/rules/sqlx-database.md` — query macro patterns and type mapping.
- `migrations/0001_initial_schema.sql` — the canonical reference for table, index, and constraint style in this project.
- `docs/PURGE_AND_STORAGE.md` — context for blob/document lifecycle if your migration touches those tables.
