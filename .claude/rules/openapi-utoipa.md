---
paths:
  - "src/api/**/*.rs"
---

# OpenAPI with utoipa (v5)

## Handler Annotations
Every public endpoint must have `#[utoipa::path(...)]`:
```rust
#[utoipa::path(
    get,
    path = "/api/resource/{id}",
    params(("id" = Uuid, Path, description = "Resource ID")),
    responses(
        (status = 200, description = "Found", body = ResourceResponse),
        (status = 404, description = "Not found"),
        (status = 401, description = "Unauthorized")
    ),
    security(("bearer_auth" = [])),
    tag = "resources"
)]
```

## Schema Derivation
- Derive `ToSchema` on all request/response types.
- Use doc comments (`///`) on fields — they become OpenAPI descriptions.
- Use `#[schema(example = "value")]` for examples, `#[schema(nullable)]` for `Option<T>`.

## Best Practices
- Separate request and response schemas (e.g., `CreateRequest` vs `Response`).
- Group endpoints with `tag` for organized documentation.
- Keep `components(schemas(...))` in sync with types used in `responses(body = ...)`.
- Serve with `Scalar::with_url("/docs", ApiDoc::openapi())`.
