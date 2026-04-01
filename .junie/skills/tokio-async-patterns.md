# Tokio & Async Rust Patterns

## Runtime

- Use `#[tokio::main]` for the entry point with the multi-threaded runtime (default).
- Use `#[tokio::main(flavor = "current_thread")]` for single-threaded runtime (tests, simple tools).
- Use `#[tokio::test]` for async test functions.
- The runtime manages a thread pool — avoid blocking it with synchronous I/O.

## Spawning Tasks

```rust
// Spawn a concurrent task on the runtime
let handle = tokio::spawn(async move {
    do_work().await
});
let result = handle.await?; // JoinHandle returns Result<T, JoinError>
```
- `tokio::spawn` requires the future to be `Send + 'static`.
- Use `tokio::task::spawn_blocking` for CPU-heavy or blocking operations:
  ```rust
  let result = tokio::task::spawn_blocking(move || {
      compute_hash(&data) // synchronous, CPU-bound
  }).await?;
  ```
- Never call `.await` inside `spawn_blocking` — it's a sync context.

## Concurrency Patterns

### Join multiple futures
```rust
let (a, b, c) = tokio::join!(fetch_a(), fetch_b(), fetch_c());
// All three run concurrently; waits for all to complete.
```

### Select first to complete
```rust
tokio::select! {
    val = future_a() => { /* a finished first */ }
    val = future_b() => { /* b finished first */ }
}
```
- `select!` cancels the losing branch — ensure futures are cancel-safe.

### Bounded concurrency
```rust
use tokio::sync::Semaphore;
let sem = Arc::new(Semaphore::new(10)); // max 10 concurrent
for item in items {
    let permit = sem.clone().acquire_owned().await.unwrap();
    tokio::spawn(async move {
        process(item).await;
        drop(permit);
    });
}
```

## Channels

- `tokio::sync::mpsc` – multi-producer, single-consumer (most common for task communication).
- `tokio::sync::oneshot` – single-value, single-use (request-response pattern).
- `tokio::sync::broadcast` – multi-producer, multi-consumer (pub/sub).
- `tokio::sync::watch` – single-producer, multi-consumer (latest-value broadcast, e.g., config changes).
- Always use bounded channels (`mpsc::channel(capacity)`) to apply backpressure.

## Synchronization

- `tokio::sync::Mutex` – async-aware mutex; use when holding lock across `.await` points.
- `std::sync::Mutex` – use when lock is held briefly with no `.await` inside (lower overhead).
- `tokio::sync::RwLock` – async read-write lock; prefer when reads vastly outnumber writes.
- `tokio::sync::Notify` – signal between tasks without data.
- Avoid holding any lock across `.await` when possible — it blocks other tasks.

## Timeouts & Cancellation

```rust
use tokio::time::{timeout, Duration};
match timeout(Duration::from_secs(5), some_future()).await {
    Ok(result) => { /* completed in time */ }
    Err(_) => { /* timed out */ }
}
```
- Use `tokio::time::sleep` instead of `std::thread::sleep` — never block the runtime.
- Use `CancellationToken` from `tokio-util` for cooperative cancellation across tasks.

## Graceful Shutdown

```rust
use tokio::signal;
let ctrl_c = signal::ctrl_c();
tokio::select! {
    _ = ctrl_c => { tracing::info!("Shutting down..."); }
    _ = server_future => { /* server exited */ }
}
// Clean up resources here
```
- Use `tokio::sync::watch` or `CancellationToken` to propagate shutdown to background tasks.
- Axum's `serve(...).with_graceful_shutdown(signal)` integrates directly.

## Common Pitfalls

- **Blocking the runtime**: Never use `std::thread::sleep`, synchronous file I/O, or CPU-heavy loops in async context. Use `spawn_blocking` instead.
- **Holding locks across await**: Causes other tasks to block. Restructure to release lock before `.await`.
- **Unbounded channels**: Can cause OOM under load. Always use bounded channels.
- **Forgetting to await**: `tokio::spawn` returns a `JoinHandle` — if you drop it, the task still runs but errors are silently lost.
- **Send bounds**: Futures passed to `tokio::spawn` must be `Send`. Avoid holding `Rc`, `Cell`, or non-Send types across `.await`.

## Tracing Integration

- Use `tracing` crate (not `log`) for structured, async-aware logging.
- Instrument async functions with `#[tracing::instrument]` for automatic span creation.
- Use `tracing::info!`, `tracing::error!`, etc. with structured fields:
  ```rust
  tracing::info!(user_id = %id, action = "login", "User logged in");
  ```
- Configure with `tracing-subscriber` and `EnvFilter` for runtime log level control via `RUST_LOG`.
