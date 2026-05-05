---
name: api-endpoint
description: Add a new HTTP API endpoint to the PackDMS Axum service — handler, request/response DTOs, OpenAPI annotations, route registration, and integration test. Use this whenever the user asks to "add an endpoint", "expose X over the API", "create a route for...", "add a new POST/GET/PATCH/DELETE", or describes a new operation that should be reachable via HTTP. Also trigger when extending an existing resource with a new sub-route.
---

# Add a new API endpoint

This skill captures the full path of adding an endpoint to PackDMS so nothing important is missed: the handler, the DTOs, the OpenAPI metadata, the router wiring, the schema registration, and a test.

PackDMS is a Rust/Axum 0.8 service with `utoipa` 5 for OpenAPI, `sqlx` for Postgres, and JWT auth. Endpoints live in `src/api/`, DTOs in `src/api/types.rs`, and the OpenAPI doc + router are assembled in `src/api/mod.rs`. Errors are returned as RFC 9457 `ProblemDetails` (see `src/api/error.rs`).

## Decide where the handler goes

PackDMS groups handlers by resource: `src/api/documents.rs`, `versions.rs`, `acl.rs`, `audit.rs`. Pick the file that matches the resource. If the resource is genuinely new, create `src/api/<resource>.rs` and add `mod <resource>;` to `src/api/mod.rs`.

## Write the DTOs

Add request/response types in `src/api/types.rs` (or a resource-specific types file if the module has one). The codebase keeps request and response types separate even when they look similar — don't reuse a domain model directly in the API. The reason: domain models change for storage reasons, and the API contract should be insulated.

Each DTO needs:
- `#[derive(Serialize, Deserialize, ToSchema)]` for response types; request-only types can drop `Serialize`.
- Doc comments (`///`) on every field — these become OpenAPI descriptions.
- `#[schema(example = ...)]` on the struct for a representative example payload.
- `#[schema(value_type = Object, example = ...)]` for `serde_json::Value` fields, since utoipa can't infer a schema for arbitrary JSON.
- `From<DomainModel>` impl when the response is derived from a domain type.

```rust
/// Request to create a new widget.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(example = json!({"name": "Acme", "tier": "gold"}))]
pub struct CreateWidgetRequest {
    /// Display name of the widget.
    pub name: String,
    /// Service tier (free, gold, platinum).
    pub tier: String,
}
```

## Write the handler

Handlers are async functions returning `Result<T, ProblemDetails>`. Body-consuming extractors (`Json`, `Multipart`) must come **last** in the parameter list — Axum 0.8 enforces this.

The standard parameter order in this codebase is: `State<Arc<AppState>>`, `JwtAuth(auth)`, `Path(...)`, `Query(...)`, then `Json(req)`.

```rust
/// Short summary that appears in the OpenAPI operation list.
///
/// Longer description shown in the operation detail view.
#[utoipa::path(
    post,
    path = "/widgets",
    request_body = CreateWidgetRequest,
    responses(
        (status = 201, description = "Widget created", body = WidgetResponse),
        (status = 400, description = "Bad request", body = ProblemDetails),
        (status = 401, description = "Unauthorized", body = ProblemDetails),
        (status = 403, description = "Forbidden", body = ProblemDetails)
    ),
    security(("bearerAuth" = ["widget:write"])),
    tag = "Widgets"
)]
pub async fn create_widget(
    State(state): State<Arc<AppState>>,
    JwtAuth(auth): JwtAuth,
    Json(req): Json<CreateWidgetRequest>,
) -> Result<(StatusCode, Json<WidgetResponse>), ProblemDetails> {
    auth.require_scope("widget:write")?;
    // ... validate input, run DB work in a transaction, write audit, commit ...
    Ok((StatusCode::CREATED, Json(widget.into())))
}
```

Things this codebase always does, and the reasons why:

- **Auth scope check first** via `auth.require_scope("...")` — fail fast before touching the database.
- **Validation returns `bad_request("...")`** from `super::error` — don't propagate raw validation errors.
- **DB work inside `pool.begin().await`** when there's more than one statement, plus an `AuditRepo::create` insert, then `tx.commit()`. Auto-rollback on drop is the safety net.
- **Map sqlx/storage errors with `.map_err(internal)`** — `internal` logs the error and returns a 500 without leaking details to the client.
- **Permission/ACL enforcement** uses `enforce_permission(&mut tx, doc_id, auth.user_id, Permission::Write)` from `super::acl_guard` for document-scoped operations.
- **Status codes**: 201 for creation, 204 for deletion, 200 with body otherwise. Use `(StatusCode::CREATED, Json(...))` tuples to override the default 200.

## Wire the route in `src/api/mod.rs`

Two edits are needed in `src/api/mod.rs`:

1. **Add the handler to the `paths(...)` list** in the `#[derive(OpenApi)]` block, e.g. `widgets::create_widget,`.
2. **Add any new schema types to `components(schemas(...))`** — both request and response types. utoipa won't auto-discover them.
3. **Register the route** in the `router(state)` function. Group related routes on the same `.route()` call when they share a path:

```rust
.route(
    "/widgets",
    post(widgets::create_widget).get(widgets::list_widgets),
)
.route("/widgets/{id}", get(widgets::get_widget))
```

If the resource is new, also add a `tags(...)` entry in the `#[openapi(...)]` block with a one-sentence description — that's what shows in the Scalar UI sidebar.

For uploads larger than the default body limit, attach `.route_layer(DefaultBodyLimit::max(N))` per-route as `versions::upload_version` does.

## Write an integration test

Every new endpoint gets at least one happy-path integration test in `tests/`. The `acl_enforcement.rs` file has a `Harness` struct that's the right model to copy from — it spins up an in-memory storage, generates an RSA keypair, builds a `JwkSet`, runs migrations, and constructs `AppState`. Reuse that pattern; don't roll your own.

Tests use `tower::ServiceExt::oneshot` against a `Router` — no real server. The integration-test skill has more detail on the harness shape.

A minimal happy-path test:

```rust
let app = api::router(harness.state.clone());
let token = harness.mint_token(user_id, &["widget:write"]);
let response = app
    .oneshot(
        Request::builder()
            .method("POST")
            .uri("/widgets")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"acme","tier":"gold"}"#))?,
    )
    .await?;
assert_eq!(response.status(), StatusCode::CREATED);
```

Cover at least: happy path, missing/invalid auth (401), missing scope (403), validation failure (400), and the not-found case if the endpoint takes an ID.

## Final checks

Before declaring the endpoint done:

- `cargo fmt`
- `cargo clippy -- -D warnings` — the project enforces zero warnings.
- `cargo build` — utoipa errors at compile time when schemas are missing or paths reference unknown handlers.
- `cargo test` (or `cargo test --test <new_test_file>` for the new one).
- Open `http://localhost:8080/docs` and verify the operation appears under the right tag with the expected request/response schemas. utoipa silently omits handlers that aren't registered in `paths(...)`, so the visual check catches forgotten registrations.

## Related project docs

- `src/api/error.rs` — `ProblemDetails` and the `bad_request`/`not_found`/`forbidden`/`unauthorized`/`internal` helpers.
- `.claude/rules/axum-api.md` — Axum 0.8 routing & extractor rules.
- `.claude/rules/openapi-utoipa.md` — utoipa annotation reference.
- `docs/RIGHTS_SCOPES_ACL.md` — when to use `auth.require_scope(...)` vs `enforce_permission(...)`.
