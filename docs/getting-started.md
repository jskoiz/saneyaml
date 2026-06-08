# Getting started

Install, parse YAML into your own types, and emit it back. About five minutes.

> Snippets below elide the enclosing function and use `?`; assume each runs in a
> function returning `saneyaml::Result<()>`.

## Install

```toml
[dependencies]
saneyaml = "0.3.0"
serde = { version = "1", features = ["derive"] }
```

Pure Rust, no C bindings, `#![forbid(unsafe_code)]`. MSRV 1.88.

## Parse into a struct

The common case — load a config straight into your own types with
`#[derive(Deserialize)]`:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    name: String,
    port: u16,
    tags: Vec<String>,
}

let cfg: Config = saneyaml::from_str("\
name: web
port: 8080
tags: [http, public]
")?;

assert_eq!(cfg.port, 8080);
assert_eq!(cfg.tags, ["http", "public"]);
```

Three entry points, same behavior — pick by what you hold:

```rust
let a: Config = saneyaml::from_str(text)?;     // &str
let b: Config = saneyaml::from_slice(bytes)?;  // &[u8]
let c: Config = saneyaml::from_reader(file)?;  // impl std::io::Read
```

## Parse into a dynamic value

When you don't know the shape ahead of time, deserialize into `Value` and walk
it:

```rust
let v: saneyaml::Value = saneyaml::from_str("name: web\nport: 8080\n")?;

assert_eq!(v["name"].as_str(), Some("web"));
assert_eq!(v["port"].as_u64(), Some(8080));
```

`Value` mirrors `serde_yaml::Value`: `as_str`, `as_i64`, `as_bool`,
`as_sequence`, `as_mapping`, indexing by key or position, and `get` for a
non-panicking lookup. See the [Cookbook](cookbook.md#work-with-value) for
mutation and patching.

## Emit

Serialize any `Serialize` value to YAML:

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Config { name: String, port: u16 }

let cfg = Config { name: "web".into(), port: 8080 };

let text = saneyaml::to_string(&cfg)?;         // -> String
// name: web
// port: 8080

saneyaml::to_writer(std::io::stdout(), &cfg)?; // writes to any impl std::io::Write
```

The default writer produces clean, deterministic YAML. To tune layout — sort
keys, force quoting, flow vs block — use `EmitOptions` and
`to_string_with_options`; see the [Cookbook](cookbook.md#control-emitted-yaml).

## Entry-point cheat sheet

| You have… | You want… | Call |
|---|---|---|
| `&str` / `&[u8]` / reader | one typed value | `from_str` / `from_slice` / `from_reader` |
| a multi-document stream | a `Vec<T>` | `from_documents_str` / `_slice` / `_reader` |
| a multi-document stream | one document at a time | `Deserializer::from_str(...)`, then iterate |
| a `Serialize` value | a `String` or writer | `to_string` / `to_writer` |
| YAML text | the raw structure | `parse_str` → `Node`, or read into `Value` |
| non-default schema or limits | any of the above | `LoadOptions::…().from_str(...)` |

## Where to next

- **[Cookbook](cookbook.md)** — multi-document streams, enums, anchors, `Value`
  patching, emitter options.
- **[Schema modes](schema-modes.md)** — control how scalars like `NO`, `on`, and
  `0123` resolve.
- **[Migrating from serde_yaml](MIGRATION.md)** — if you're replacing it.
