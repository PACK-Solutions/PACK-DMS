---
paths:
  - "src/**/*.rs"
---

# Error Handling

## Rules
- **Never `.unwrap()` in production code**. Use `?`, `.expect("reason")` for invariants, or handle explicitly.
- Propagate errors up with `?` — let the caller decide.
- Log at the boundary (HTTP handlers), not where errors originate.

## Patterns
- Application errors: `anyhow::Result<T>` with `.context("message")`.
- Domain errors: custom enums with `thiserror` derives and `#[from]` for auto-conversion.
- API errors: implement `IntoResponse` mapping domain variants → HTTP status codes (see `api/error.rs`).
- Optional chaining: `repo.find(id).await?.ok_or_else(|| DomainError::NotFound(...))?;`
