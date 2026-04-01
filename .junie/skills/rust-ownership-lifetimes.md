# Rust Ownership, Borrowing & Lifetimes

## Ownership Rules

1. Each value has exactly one owner.
2. When the owner goes out of scope, the value is dropped.
3. Ownership can be transferred (moved) or borrowed (referenced).

## Borrowing

- `&T` – shared/immutable reference. Multiple allowed simultaneously.
- `&mut T` – exclusive/mutable reference. Only one at a time, no shared refs coexist.
- References must always be valid (no dangling pointers).

### Common patterns
```rust
fn process(data: &[u8]) { /* read-only access */ }
fn modify(data: &mut Vec<u8>) { data.push(0); }

// Reborrowing: &mut T can be temporarily borrowed as &T
fn inspect(items: &mut Vec<Item>) {
    let count = items.len(); // implicit reborrow as &Vec
    items.push(new_item);    // back to &mut
}
```

## Move Semantics

- Assignment and function calls move non-`Copy` types:
  ```rust
  let s1 = String::from("hello");
  let s2 = s1; // s1 is moved, no longer usable
  ```
- `Copy` types (integers, floats, `bool`, `char`, tuples of `Copy` types) are copied instead.
- Use `.clone()` for explicit deep copies when you need to keep the original.

## Lifetimes

- Lifetimes ensure references don't outlive the data they point to.
- Most lifetimes are inferred (elision rules). Annotate only when the compiler asks.

### Elision rules (automatic)
1. Each input reference gets its own lifetime.
2. If there's exactly one input lifetime, it's assigned to all outputs.
3. If `&self` or `&mut self` is an input, its lifetime is assigned to outputs.

### When to annotate
```rust
// Compiler can't infer: which input does the output borrow from?
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// Struct holding a reference needs a lifetime
struct Parser<'a> {
    input: &'a str,
}
```

## Smart Pointers

- `Box<T>` – heap allocation, single owner. Use for recursive types or large stack values.
- `Rc<T>` – reference-counted, single-threaded shared ownership. Use sparingly.
- `Arc<T>` – atomic reference-counted, thread-safe shared ownership. Common in async code.
- `Cow<'a, T>` – clone-on-write. Borrows when possible, clones only when mutation needed:
  ```rust
  fn process(input: Cow<'_, str>) -> Cow<'_, str> {
      if input.contains("bad") {
          Cow::Owned(input.replace("bad", "good"))
      } else {
          input // no allocation
      }
  }
  ```

## Interior Mutability

- `Cell<T>` – for `Copy` types; get/set without `&mut`.
- `RefCell<T>` – runtime borrow checking; panics on violation. Single-threaded only.
- `Mutex<T>` / `RwLock<T>` – thread-safe interior mutability (see tokio-async-patterns).
- `OnceCell<T>` / `OnceLock<T>` – write-once, read-many (lazy initialization).

## Common Pitfalls

- **Borrowing from a temporary**: `let r = &String::from("temp");` — the `String` is dropped immediately.
- **Mutable borrow while iterating**: Can't modify a collection while iterating over it. Use indices, `retain`, or collect-then-modify.
- **Moving out of a reference**: Can't move owned data out of `&T`. Use `.clone()`, `std::mem::take`, or `Option::take`.
- **Lifetime too short**: If a function returns a reference, it must borrow from an input — not from a local variable.
