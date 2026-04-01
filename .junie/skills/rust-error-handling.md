# Rust Error Handling Patterns

## The Error Hierarchy

### Application errors (`anyhow`)
- Use `anyhow::Result<T>` for application-level code (main, setup, scripts).
- Chain context with `.context("message")` or `.with_context(|| format!("..."))`.
- `anyhow::bail!("msg")` for early returns; `anyhow::ensure!(cond, "msg")` for assertions.
- Best for: binaries, CLI tools, top-level orchestration.

### Library/domain errors (custom types)
- Define domain-specific error enums for business logic:
  ```rust
  #[derive(Debug)]
  enum DomainError {
      NotFound(String),
      Conflict(String),
      Validation(String),
      Unauthorized,
  }
  ```
- Implement `std::fmt::Display` and `std::error::Error` for custom errors.
- Use `thiserror` crate to derive these implementations:
  ```rust
  #[derive(Debug, thiserror::Error)]
  enum DomainError {
      #[error("resource not found: {0}")]
      NotFound(String),
      #[error("conflict: {0}")]
      Conflict(String),
      #[error(transparent)]
      Database(#[from] sqlx::Error),
  }
  ```

## Error Conversion

- Implement `From<SourceError> for TargetError` for automatic `?` conversion.
- With `thiserror`, use `#[from]` attribute for automatic `From` implementations.
- With `anyhow`, any `std::error::Error` type converts automatically via `?`.

## Axum Error Mapping

Map domain errors to HTTP responses:
```rust
impl IntoResponse for DomainError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            DomainError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            DomainError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            DomainError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            DomainError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}
```

## Best Practices

- **Never use `.unwrap()` in production code** — use `?`, `.expect("reason")`, or handle explicitly.
- **Use `.expect("reason")` only for invariants** that indicate programmer error if violated.
- **Propagate errors up** with `?` — let the caller decide how to handle them.
- **Log at the boundary** — log errors where they're handled (e.g., in HTTP handlers), not where they originate.
- **Don't over-wrap errors** — avoid `map_err` chains that lose the original error. Use `#[from]` or `.context()`.
- **Match on error variants** when recovery is possible; propagate when it's not.
- **Use `Result<(), E>` for fallible operations** that don't return a value.

## Patterns

### Fallible constructors
```rust
impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let port = std::env::var("PORT")?.parse::<u16>()?;
        Ok(Self { port })
    }
}
```

### Optional chaining with errors
```rust
let user = repo.find_by_id(id).await?
    .ok_or_else(|| DomainError::NotFound(format!("user {id}")))?;
```

### Collecting results
```rust
let results: Result<Vec<_>, _> = items.iter().map(|i| process(i)).collect();
let values = results?;
```
