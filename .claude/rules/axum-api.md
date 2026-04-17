---
paths:
  - "src/api/**/*.rs"
  - "src/admin/**/*.rs"
---

# Axum API Patterns (v0.8)

## Routing
- `Router::new().route("/path", get(handler).post(handler))` — macro-free.
- `Router::nest("/prefix", sub_router)` for modular organization.
- Apply middleware with `.layer()`. Layer ordering: last `.layer()` runs first.

## Extractors
- Body-consuming extractors (`Json`, `Multipart`) must be the **last** parameter.
- Common: `Path<T>`, `Query<T>`, `Json<T>`, `State<T>`, `Extension<T>`, `HeaderMap`.
- Custom extractors: implement `FromRequestParts<S>` (no body) or `FromRequest<S>` (with body).

## Handlers
- Async functions taking extractors, returning `impl IntoResponse`.
- Return types: `Json<T>`, `(StatusCode, impl IntoResponse)`, `Result<T, AppError>`.
- Use `#[debug_handler]` during development for better compile errors.

## State
- Prefer `State<T>` over `Extension<T>` for compile-time safety.
- Wrap shared mutable state in `Arc<Mutex<T>>` or `Arc<RwLock<T>>`.

## Error Handling
- `AppError` enum implements `IntoResponse`, mapping variants to HTTP status codes.
- Handler return: `Result<Json<T>, AppError>`.
- Never let panics escape handlers.

## Testing
```rust
let app = Router::new().route("/", get(handler));
let response = app.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
```
