---
name: code-review
description: Review pending code changes against PackDMS conventions — error handling, OpenAPI annotations, SQLx patterns, ACL enforcement, async correctness, test coverage, and the Microsoft Pragmatic Rust Guidelines. Use whenever the user asks to "review my changes", "check this PR", "is this code OK?", "look over what I just did", "before I commit...", or describes wanting a second pass before merging. Also trigger when the user wants critique of a specific function, module, or diff — even without the word "review".
---

# Review changes against PackDMS conventions

This skill is a structured review pass tailored to PackDMS. It is *not* a generic Rust review — it focuses on the specific conventions this codebase enforces, plus the cross-cutting concerns (security, ACL, OpenAPI consistency) that are easy to miss.

For deep multi-step reviews where independent analysis is valuable, prefer spawning the existing `review` subagent (`.claude/agents/review.md`) instead — it's configured for that. Use this skill for inline, conversational reviews where you walk through the diff with the user.

## Step 1 — Get the diff

Always start from the actual changes, not from memory of what was just edited. The user's mental model and the diff can diverge.

```bash
git status
git diff                  # unstaged
git diff --staged         # staged
git diff main...HEAD      # whole branch vs main
```

Read each modified file in full when context matters — a changed function may be called from places the diff doesn't show. Use `grep` for callers before flagging an API change as safe.

## Step 2 — Walk the checklist

Group findings as **Critical** (must fix — correctness/security), **Warning** (should fix — convention/style), **Suggestion** (nice-to-have). Tell the user which severity each item is so they can prioritize.

### Correctness & safety

- [ ] No `.unwrap()` or `.expect()` on `Result`/`Option` in production paths. `.expect("reason")` is acceptable only for true invariants (poisoned locks, statically-known config). The convention in this codebase is to propagate with `?` and let the caller decide.
- [ ] Errors from sqlx, storage, and auth are mapped through `api::error::internal` (logs server-side, returns generic 500) rather than leaked to the client.
- [ ] User-facing errors use the right helper: `bad_request`, `not_found`, `forbidden`, `unauthorized`. A 500 for a validation failure is a regression.
- [ ] Database queries are parameterized (`sqlx::query!`/`query_as!` macros, or `query(..).bind(..)`). No `format!`-built SQL strings — that's an injection bug regardless of what's interpolated.
- [ ] Multi-statement DB work is wrapped in `pool.begin()` → `tx.commit()`. Audit log inserts share the transaction with the operation they describe.
- [ ] Async code never blocks the runtime. CPU-bound work (hashing, compression, parsing large blobs) goes through `tokio::task::spawn_blocking`. File I/O uses `tokio::fs`, not `std::fs`.
- [ ] No `panic!()`, `todo!()`, or `unimplemented!()` left behind in production paths.

### ACL and authorization

- [ ] Endpoints that touch a document call `enforce_permission(&mut tx, doc_id, auth.user_id, Permission::X)` from `api::acl_guard`, with the right `Permission` variant for the operation.
- [ ] Scope checks (`auth.require_scope("document:write")`) happen before any DB work — fail fast.
- [ ] Writes to ACL/audit tables match the patterns in `docs/RIGHTS_SCOPES_ACL.md`. Owner gets implicit admin via the auto-created ACL row in `create_document`; new endpoints that bypass that path need to handle ownership explicitly.
- [ ] No path traversal or unchecked user input in storage keys (the storage adapter constructs keys from internal IDs — confirm new paths follow that).

### API design (handlers in `src/api/`)

- [ ] Every public handler has a `#[utoipa::path(...)]` annotation: method, path, params, request_body if applicable, all response status codes with body types, `security(("bearerAuth" = [...]))`, and a `tag`.
- [ ] Request and response DTOs are separate types (`CreateXRequest` vs `XResponse`). Domain models are not exposed directly.
- [ ] DTOs derive `Serialize`/`Deserialize` and `ToSchema`. Fields have `///` doc comments and meaningful `#[schema(example = ...)]`.
- [ ] New schemas are registered in `components(schemas(...))` in `src/api/mod.rs`. New paths are listed in `paths(...)`. Forgetting either silently omits the operation from the OpenAPI doc.
- [ ] Body-consuming extractors (`Json`, `Multipart`) are the **last** parameter in handler signatures.
- [ ] Status codes match REST conventions: 201 for creation (with `(StatusCode::CREATED, Json(...))`), 204 for deletion (no body), 200 with body otherwise.

### SQLx and migrations

- [ ] New schema changes have a corresponding migration file in `migrations/` named `YYYYMMDDHHMMSS_description.sql`.
- [ ] DDL is idempotent: `CREATE TABLE IF NOT EXISTS`, `ADD COLUMN IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`. The test suite re-applies migrations against an existing DB.
- [ ] Existing applied migrations are **not modified** — fixes go in a new migration.
- [ ] Type mappings line up: `Uuid`↔`UUID`, `DateTime<Utc>`↔`TIMESTAMPTZ`, `serde_json::Value`↔`JSONB`, nullable columns ↔ `Option<T>`.
- [ ] `RETURNING` clauses are used to avoid extra `SELECT`s after `INSERT`/`UPDATE` where applicable.

### Performance and resource use

- [ ] No needless `.clone()` — borrowing or moving is usually cheaper. Cloning a `String`/`Vec` in a hot path is worth flagging.
- [ ] No N+1 queries. If the diff loads N documents and then loops to fetch related data, push the join into SQL.
- [ ] Multipart upload paths stream rather than buffering the full body where size matters; check `DefaultBodyLimit` configuration on the route.
- [ ] Long-running CPU work has yield points or runs in `spawn_blocking` — see `M-YIELD-POINTS` in `rust-guidelines.txt`.

### Tests

- [ ] New or changed behavior has integration tests in `tests/`. Pure domain logic may have unit tests in `#[cfg(test)]` modules instead.
- [ ] Tests cover happy path, auth failure (401), authorization failure (403), validation failure (400), and not-found (404) where each applies.
- [ ] Tests use the `Harness` pattern from `tests/acl_enforcement.rs` (or extend it). No reinvented JWK/router setup inline.
- [ ] Tests use unique IDs/emails (`Uuid::new_v4()`) — no hardcoded values that collide across runs.

### Style and conventions

- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy -- -D warnings` is clean. The project enforces zero warnings; suppressions need `#[expect(..., reason = "...")]` (per `M-LINT-OVERRIDE-EXPECT`), not `#[allow(...)]`.
- [ ] `snake_case` for functions and variables; `PascalCase` for types and traits.
- [ ] Module boundaries respected: `domain/` has no framework deps (no `axum`, no `sqlx` types in signatures); `api/` translates between HTTP and domain; `infra/` does the I/O.
- [ ] No `pub use foo::*` glob re-exports outside HAL-style platform shims.
- [ ] Error types are structured (struct or enum with `thiserror`), not stringly-typed.

### Logging and observability

- [ ] Structured logging with named fields, not `format!`-assembled messages: `tracing::info!(file.path = %path, "file processed")`. The reason: the message template is cheap to render at log time and properties are filterable.
- [ ] No sensitive data in logs (emails, tokens, file contents). Redact when you must log a user identifier.
- [ ] Errors are logged at the boundary (`api/error.rs::internal`), not at every layer — duplicate logs add noise without information.

### Documentation

- [ ] Public functions and types in library code have `///` doc comments. Handlers' doc comments become OpenAPI descriptions — make them user-facing.
- [ ] First sentence of a doc comment is a single-line summary under ~15 words (`M-FIRST-DOC-SENTENCE`).
- [ ] Magic numbers are either named constants or have a comment explaining the choice (`M-DOCUMENTED-MAGIC`).

## Step 3 — Run the validation pipeline

After reviewing the diff, suggest the user run (or run it yourself if appropriate):

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo build
cargo test
```

If any step fails, the failure is itself a critical review finding. Fix the underlying issue rather than suppressing the warning.

## Step 4 — Report

Structure the report as:

```
## Critical
- <file>:<line> — <issue>. <why it matters>. <suggested fix>

## Warnings
- ...

## Suggestions
- ...

## Validation
✅ fmt   ✅ clippy   ✅ build   ✅ tests
```

If a finding is a tradeoff rather than a clear fix, present both sides and let the user decide — don't pretend there's only one right answer when reasonable engineers would disagree.

## Related project resources

- `.claude/agents/review.md` — the dedicated review subagent for deeper passes.
- `.claude/agents/validate.md` — runs the full pipeline and reports.
- `rust-guidelines.txt` — Microsoft Pragmatic Rust Guidelines, the source for many of the conventions above (M-* identifiers).
- `docs/RIGHTS_SCOPES_ACL.md` — the authoritative reference for what counts as a permission violation.
- `docs/LIFECYCLE.md`, `docs/VERSIONING.md`, `docs/PURGE_AND_STORAGE.md`, `docs/METADATA.md` — domain rules a reviewer should check business logic against.
