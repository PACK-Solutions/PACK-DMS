# Serde Serialization Patterns

## Basics

- Derive `Serialize` and `Deserialize` on all DTOs:
  ```rust
  #[derive(serde::Serialize, serde::Deserialize)]
  struct User { name: String, email: String }
  ```
- Serde works with JSON (`serde_json`), TOML, YAML, MessagePack, and many other formats.

## Common Attributes

### Container attributes (on struct/enum)
- `#[serde(rename_all = "camelCase")]` – convert field names (also: `snake_case`, `SCREAMING_SNAKE_CASE`, `kebab-case`, `PascalCase`).
- `#[serde(deny_unknown_fields)]` – reject unknown JSON keys (strict parsing).
- `#[serde(default)]` – use `Default::default()` for missing fields.
- `#[serde(tag = "type")]` – internally tagged enum representation.
- `#[serde(tag = "type", content = "data")]` – adjacently tagged enum.
- `#[serde(untagged)]` – untagged enum (tries each variant in order).

### Field attributes
- `#[serde(rename = "fieldName")]` – rename a single field.
- `#[serde(default)]` – use field type's `Default` if missing.
- `#[serde(default = "path::to::fn")]` – custom default function.
- `#[serde(skip)]` – skip field in both serialization and deserialization.
- `#[serde(skip_serializing_if = "Option::is_none")]` – omit `None` values from output.
- `#[serde(flatten)]` – inline nested struct fields into parent.
- `#[serde(with = "module")]` – custom serialize/deserialize via a module.
- `#[serde(deserialize_with = "fn")]` / `#[serde(serialize_with = "fn")]` – custom functions.

## Enum Representations

```rust
// Externally tagged (default): {"variant_name": { ...data... }}
// Internally tagged:           {"type": "variant_name", ...data... }
// Adjacently tagged:           {"type": "variant_name", "data": { ...data... }}
// Untagged:                    { ...data... } (tries each variant)
```
- Prefer internally tagged (`#[serde(tag = "type")]`) for API responses — cleaner JSON.
- Use `#[serde(rename = "...")]` on variants for custom discriminator values.

## Working with JSON

```rust
// Serialize
let json_string = serde_json::to_string(&value)?;
let json_pretty = serde_json::to_string_pretty(&value)?;
let json_value = serde_json::to_value(&value)?;

// Deserialize
let parsed: MyStruct = serde_json::from_str(&json_string)?;
let parsed: MyStruct = serde_json::from_value(json_value)?;

// Dynamic JSON
let v: serde_json::Value = serde_json::from_str(raw)?;
let name = v["name"].as_str().unwrap_or_default();
```

## Patterns for APIs

- Use separate request/response structs (don't reuse domain models directly).
- Use `Option<T>` for optional fields; combine with `#[serde(skip_serializing_if = "Option::is_none")]`.
- Use `#[serde(default)]` on request structs for backward-compatible API evolution.
- For PATCH endpoints, use `Option<Option<T>>` to distinguish "not provided" from "set to null".
- Validate after deserialization — serde handles format, not business rules.

## Performance Tips

- Use `serde_json::from_slice(&bytes)` instead of converting to `&str` first.
- Use `serde_json::StreamDeserializer` for newline-delimited JSON streams.
- For very large payloads, use `serde_json::from_reader` with a `BufReader`.
- Avoid `#[serde(flatten)]` in hot paths — it uses an intermediate `Map` and is slower.
