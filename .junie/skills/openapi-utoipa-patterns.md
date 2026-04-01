# OpenAPI with utoipa Patterns (v5)

## Setup

- Derive `#[derive(OpenApi)]` on a struct to define the API document:
  ```rust
  #[derive(OpenApi)]
  #[openapi(
      paths(list_users, get_user, create_user),
      components(schemas(User, CreateUserRequest)),
      tags((name = "users", description = "User management"))
  )]
  struct ApiDoc;
  ```
- Serve with `utoipa-scalar`: `Scalar::with_url("/docs", ApiDoc::openapi())`.

## Annotating Handlers

```rust
#[utoipa::path(
    get,
    path = "/api/users/{id}",
    params(
        ("id" = Uuid, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User found", body = User),
        (status = 404, description = "User not found"),
        (status = 401, description = "Unauthorized")
    ),
    security(("bearer_auth" = [])),
    tag = "users"
)]
async fn get_user(Path(id): Path<Uuid>) -> Result<Json<User>, StatusCode> {
    // ...
}
```

## Schema Derivation

```rust
#[derive(Serialize, Deserialize, ToSchema)]
struct User {
    /// Unique user identifier
    id: Uuid,
    /// User's display name
    #[schema(example = "John Doe", min_length = 1, max_length = 100)]
    name: String,
    /// Account creation timestamp
    created_at: DateTime<Utc>,
    /// Optional profile picture URL
    #[schema(nullable)]
    avatar_url: Option<String>,
}
```

## Common Schema Attributes

- `#[schema(example = "value")]` – example value for documentation.
- `#[schema(min_length = n, max_length = n)]` – string length constraints.
- `#[schema(minimum = n, maximum = n)]` – numeric range constraints.
- `#[schema(nullable)]` – mark field as nullable in OpenAPI.
- `#[schema(read_only)]` / `#[schema(write_only)]` – directional visibility.
- `#[schema(value_type = String)]` – override inferred type (useful for newtypes).
- `#[schema(inline)]` – inline the schema instead of using `$ref`.

## Enum Schemas

```rust
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum Status {
    Active,
    Inactive,
    Suspended,
}
```
- Simple enums become string enums in OpenAPI.
- Tagged enums with data use `oneOf` / `allOf` representations.

## Security Schemes

```rust
#[derive(OpenApi)]
#[openapi(
    // ...
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

struct SecurityAddon;
impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_default();
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}
```

## Best Practices

- Annotate **every** public endpoint with `#[utoipa::path(...)]`.
- Use doc comments (`///`) on struct fields — they become OpenAPI descriptions.
- Provide `example` values for all request/response fields.
- Use separate request and response schemas (e.g., `CreateUserRequest` vs `User`).
- Group endpoints with `tag` for organized documentation.
- Keep `components(schemas(...))` in sync with types used in `responses(body = ...)`.
