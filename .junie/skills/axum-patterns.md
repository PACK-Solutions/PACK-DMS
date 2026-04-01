# Axum Web Framework Patterns (v0.8)

## Routing

- Use `Router::new().route("/path", get(handler).post(handler))` — macro-free API.
- Nest sub-routers with `Router::nest("/prefix", sub_router)` for modular route organization.
- Use `Router::merge(other_router)` to combine routers at the same path level.
- Apply middleware with `.layer()` on `Router` or individual `MethodRouter`.
- Routes are matched in order of specificity, not insertion order.

## Extractors

- Extractors parse requests declaratively via handler function parameters.
- Order matters: `Body`-consuming extractors (`Json`, `String`, `Bytes`, `Multipart`) must be **last**.
- Common extractors:
  - `Path<T>` – URL path parameters (e.g., `Path(id): Path<Uuid>`)
  - `Query<T>` – query string parameters
  - `Json<T>` – JSON request body (requires `T: Deserialize`)
  - `State<T>` – shared application state (requires `T: Clone + Send + Sync + 'static`)
  - `Extension<T>` – values inserted by middleware
  - `HeaderMap` / `TypedHeader<T>` – request headers
  - `Multipart` – multipart form data
- Create custom extractors by implementing `FromRequestParts<S>` (no body) or `FromRequest<S>` (with body).
- Use `#[derive(FromRequestParts)]` or `#[derive(FromRequest)]` with the `macros` feature for delegation.

## Handlers

- A handler is any async function that takes extractors and returns `impl IntoResponse`.
- Use `#[debug_handler]` (from `axum-macros`) during development for better compile errors.
- Handlers can return:
  - `String`, `&str`, `Vec<u8>`, `Bytes` – plain body responses
  - `Json<T>` – JSON response (requires `T: Serialize`)
  - `(StatusCode, impl IntoResponse)` – response with custom status
  - `(StatusCode, HeaderMap, impl IntoResponse)` – full control
  - `Result<T, E>` where both `T: IntoResponse` and `E: IntoResponse`
  - `Response` – full `http::Response` for maximum control

## State Management

- Prefer `State<T>` over `Extension<T>` for compile-time safety.
- Use `#[derive(Clone)]` on your `AppState` struct.
- For substates, implement or derive `FromRef` to extract parts of the state.
- Wrap shared mutable state in `Arc<Mutex<T>>` or `Arc<RwLock<T>>`.
- Attach state with `Router::with_state(state)` or `.layer(Extension(state))`.

## Error Handling

- Axum requires handler errors to implement `IntoResponse`.
- Pattern: define an `AppError` enum that implements `IntoResponse`, mapping variants to HTTP status codes.
- Use `Result<Json<T>, AppError>` as handler return type.
- For infallible middleware, use `HandleError` to convert service errors into responses.
- Never let panics escape handlers — they abort the connection.

## Middleware

- Tower middleware: apply via `.layer(ServiceBuilder::new().layer(...))`.
- Axum-native middleware: use `axum::middleware::from_fn` or `from_fn_with_state` for async functions.
- Middleware function signature: `async fn my_middleware(request: Request, next: Next) -> Response`.
- Common tower-http layers:
  - `TraceLayer` – request/response tracing
  - `CorsLayer` – CORS headers
  - `SetRequestIdLayer` / `PropagateRequestIdLayer` – request ID propagation
  - `CompressionLayer` – response compression
  - `TimeoutLayer` – request timeouts
- Layer ordering: layers wrap outside-in (last `.layer()` call runs first).

## Response Patterns

- Implement `IntoResponse` for custom response types.
- Use `axum::response::Json` for JSON responses.
- Use `axum::response::Redirect` for redirects.
- Use `axum::response::Sse` for server-sent events.
- Set headers with `response.headers_mut()` or return tuples with `HeaderMap`.

## Testing

- Use `axum::body::Body` and `tower::ServiceExt` for testing:
  ```rust
  let app = Router::new().route("/", get(handler));
  let response = app.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  ```
- Test extractors independently by constructing mock requests.
- Use `tower::ServiceExt::oneshot` for single-request tests without starting a server.
