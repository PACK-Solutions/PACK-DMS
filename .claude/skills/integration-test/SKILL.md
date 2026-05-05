---
name: integration-test
description: Scaffold a new integration test for the PackDMS API using the project's Harness pattern — Postgres pool, in-memory blob storage, generated RSA JWKs, signed JWT, oneshot Router invocation. Use whenever the user asks to "add a test", "test this endpoint", "write integration tests for X", "verify the API behavior of...", or describes a scenario that needs HTTP-level verification. Also trigger when adding tests for ACL/permission edge cases, multipart uploads, or any flow that involves the full Axum Router.
---

# Scaffold an integration test

PackDMS integration tests live in `tests/`, run against a real Postgres (no mocks), and exercise the full Axum router via `tower::ServiceExt::oneshot`. There's no live HTTP server — `oneshot` feeds a `Request<Body>` to the in-process `Router` and returns the `Response`. That keeps tests fast and deterministic.

## When this skill applies

Use it when:
- Adding tests for a new endpoint (pair this with the `api-endpoint` skill).
- Covering an ACL/permission edge case — those almost always need the harness.
- Verifying multi-step flows (create → upload → patch → archive).
- Reproducing a bug at the HTTP layer.

For *unit* testing pure domain logic (no I/O, no DB), prefer `#[cfg(test)]` modules inside the source file. Don't drag in the harness for things that can be tested with a plain function call.

## Prerequisites

Tests require:
- A running Postgres reachable via `DATABASE_URL` (see `.env` and `docker-compose up -d`).
- The `packdms::infra::auth::issue_test_jwt` helper, which is gated behind tests/dev — it signs a JWT with the harness-generated RSA key.

Each test runs `sqlx::migrate!().run(&pool)` at startup, so the schema is always current. The DB is shared across tests and across runs — never assume an empty database. Always seed the data your test needs and use unique IDs/emails (`Uuid::new_v4()`, `format!("t+{}@example.com", Uuid::new_v4())`) to avoid collisions.

## Reuse the existing Harness, don't rebuild it

`tests/acl_enforcement.rs` defines a `Harness` struct that already sets up everything: pool, RSA keypair, JWKS, in-memory `BlobStore`, `AppState`. It also exposes:

- `Harness::new()` — full setup; returns `anyhow::Result<Harness>`.
- `harness.router()` — the same `api::router(state)` your handler will run under.
- `harness.seed_user(&["role1", "role2"])` — inserts a user, mints a signed JWT with `document:read document:write` scopes; returns `(user_id, email, token)`.
- `harness.seed_document(owner_id)` — creates a `draft` document with the owner ACL entry.
- `harness.add_acl(doc_id, principal_type, principal_id, role, permission)` — grants per-document permissions.

For new tests, **prefer adding a test function to `tests/acl_enforcement.rs`** if the harness fits, or copy the harness into a new file (`tests/<feature>_test.rs`) and extend it with helpers specific to your feature. Don't duplicate the JWK construction inline — it's noisy and easy to get wrong.

A test crate file (anything in `tests/`) is its own binary, so each file pays the harness setup cost once per test. Group related tests in the same file to amortize that cost; split files only when the helper set diverges enough to make the file unwieldy.

## Skeleton for a new test file

When the existing file isn't appropriate, start from this shape:

```rust
use axum::{body::Body, http::{Request, StatusCode}};
use packdms::api;
use tower::util::ServiceExt;

mod common;  // optional shared harness module — see below

#[tokio::test]
async fn <descriptive_test_name>() -> anyhow::Result<()> {
    let h = common::Harness::new().await?;
    let (user_id, _email, token) = h.seed_user(&["user"]).await?;

    // Arrange: seed any data the test needs.
    let doc_id = h.seed_document(user_id).await?;

    // Act: call the endpoint via the in-process router.
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/documents/{doc_id}"))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"title":"Updated"}"#))?;
    let resp = h.router().oneshot(req).await?;

    // Assert: status first, then body if relevant.
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["title"], "Updated");
    Ok(())
}
```

Note: a `tests/common/mod.rs` module is the canonical way to share helpers across multiple test files — Cargo treats `tests/common/mod.rs` as a module rather than its own test binary. If you only have one test file, just put the harness inline.

## What to cover

For an endpoint, cover at minimum:
- **Happy path** — correct auth, correct scope, valid input → expected status code and body shape.
- **Auth failures** — missing token (`401`), token without the required scope (`403`).
- **Authorization failures** — user authenticated but lacks ACL permission for the resource (`403`).
- **Validation failures** — empty/oversize input fields (`400`).
- **Not found** — operating on an unknown ID (`404`).

Multi-step flows (upload version, then download it) are best as a single test that walks the whole sequence — easier to debug than fragmented per-step tests, and the setup cost is paid once.

## Test naming

The function name is the failure message a human sees first. Use `snake_case` and describe the scenario, not the implementation: `owner_can_set_legal_hold`, `non_owner_cannot_archive`, `upload_rejects_oversize_file`. Avoid generic names like `test_documents_1` — they age badly.

## Test isolation

Tests share a database. To keep them isolated:
- Generate unique IDs and emails per test — never hardcode.
- Don't `DELETE FROM documents` or similar — other concurrent tests will break. The data accumulates; that's intentional.
- Don't depend on table counts or ordering. Filter by IDs you created in this test.

## Multipart and binary requests

For multipart upload tests (e.g., `versions::upload_version`), build the body manually:

```rust
let boundary = "----testboundary";
let body = format!(
    "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n{payload}\r\n--{boundary}--\r\n",
    payload = "hello",
);
let req = Request::builder()
    .method("POST")
    .uri(format!("/documents/{doc_id}/versions"))
    .header("authorization", format!("Bearer {token}"))
    .header("content-type", format!("multipart/form-data; boundary={boundary}"))
    .body(Body::from(body))?;
```

`tests/integration.rs` has a worked example of the upload flow.

## Asserting on response bodies

Read the body once with `axum::body::to_bytes(resp.into_body(), usize::MAX).await?`. Parse to `serde_json::Value` for ad-hoc assertions, or to a typed DTO when you want full structural validation:

```rust
let resp_dto: api::DocumentResponse = serde_json::from_slice(&bytes)?;
assert_eq!(resp_dto.status, "active");
```

Typed parsing is preferred for response shape contracts — if the schema drifts, the test fails to compile or deserialize, which is the desired signal.

## Running the tests

```bash
docker-compose up -d                                # ensure Postgres is up
cargo test                                           # all tests
cargo test --test acl_enforcement                    # one file
cargo test --test acl_enforcement owner_gets         # one test by name substring
cargo test --test <name> -- --nocapture              # see println!/tracing output
```

A failing migration aborts all tests in the binary at startup — fix the migration first.

## Related project resources

- `tests/acl_enforcement.rs` — copy-paste source for the `Harness` struct.
- `tests/integration.rs` — example of a multipart upload flow end-to-end.
- `.claude/rules/sqlx-database.md` — query patterns if your test seeds data directly via SQL.
- `src/infra/auth/` — `issue_test_jwt` and `JwtAuth` extractor; useful when you need to mint a token with non-default scopes.
