# Cookbook

Short, copy-paste recipes for the tasks that come up most. For the basics, read
[Getting started](getting-started.md) first.

> Snippets elide the enclosing function and use `?`; assume each runs in a
> function returning `saneyaml::Result<()>`.

- [Multi-document streams](#multi-document-streams)
- [Work with `Value`](#work-with-value)
- [Enums and singleton maps](#enums-and-singleton-maps)
- [Anchors and merge keys](#anchors-and-merge-keys)
- [Numbers, timestamps, and binary](#numbers-timestamps-and-binary)
- [Control emitted YAML](#control-emitted-yaml)
- [Custom tags](#custom-tags)

## Multi-document streams

Kubernetes-style `---`-separated streams. Get everything at once:

```rust
let stream = "\
kind: Service
---
kind: Deployment
";

let docs: Vec<Manifest> = saneyaml::from_documents_str(stream)?;
assert_eq!(docs.len(), 2);
```

Or process one document at a time — useful when an early document is valid but a
later one fails, and you want the good ones first:

```rust
use serde::Deserialize;

for doc in saneyaml::Deserializer::from_str(stream) {
    let manifest = Manifest::deserialize(doc)?;
    // handle manifest…
}
```

`from_documents_str` is all-or-error; the iterator yields parsed documents up to
the first error. For bounded memory on large streams, see
[Streaming](streaming.md).

## Work with `Value`

Read, mutate, and patch a dynamic document. Indexing returns a null sentinel for
missing paths, so chains don't panic:

```rust
let mut v: saneyaml::Value = saneyaml::from_str("\
services:
  api:
    image: nginx:1.25
    ports: [80]
")?;

// read
assert_eq!(v["services"]["api"]["image"].as_str(), Some("nginx:1.25"));

// patch in place
v["services"]["api"]["image"] = saneyaml::Value::from("nginx:1.27");

// mutate a sequence
if let Some(ports) = v["services"]["api"]["ports"].as_sequence_mut() {
    ports.push(saneyaml::Value::from(443));
}
```

`Value::from` accepts strings, bools, every integer/float width, `Mapping`, and
`Vec<Value>`. Build maps with `Mapping`, which keeps insertion order and offers
the full `entry` / `get` / `insert` / `remove` API.

To convert between typed values and `Value` without going through text, use
`to_value` and `from_value`:

```rust
let value = saneyaml::to_value(&cfg)?;        // T -> Value
let cfg: Config = saneyaml::from_value(value)?; // Value -> T
```

> `from_value` is spanless: it won't coerce a number or bool into a `String`
> target, because the original spelling is gone after the value is built. Read
> with `from_str` / `from_slice` when a string field must preserve source text
> like `1_000` or `FALSE`.

## Enums and singleton maps

Data-carrying enums round-trip as YAML tags by default (`!Variant value`). For
the `serde_yaml` single-key-map shape (`variant: value`), annotate the field:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum Action { Run(String), Skip }

#[derive(Serialize, Deserialize)]
struct Job {
    #[serde(with = "saneyaml::with::singleton_map")]
    action: Action,
}
```

Use `saneyaml::with::singleton_map_recursive` when nested enum payloads also need
the one-entry-map shape. To emit every enum as a singleton map without
annotating each field, set the emitter option globally:

```rust
use saneyaml::{EmitOptions, EnumRepresentation};

let opts = EmitOptions::structural()
    .with_enum_representation(EnumRepresentation::SingletonMap);
let text = saneyaml::to_string_with_options(&job, opts)?;
```

## Anchors and merge keys

`&anchor` / `*alias` and the `<<` merge key are expanded for you when loading
into `Node` or `Value` — you read the effective, merged result:

```rust
let v: saneyaml::Value = saneyaml::from_str("\
defaults: &defaults
  retries: 3
service:
  <<: *defaults
  name: api
")?;

assert_eq!(v["service"]["retries"].as_u64(), Some(3));
assert_eq!(v["service"]["name"].as_str(), Some("api"));
```

Explicit keys override merged ones, and earlier entries in a merge list win.
`Value::apply_merge()` is also available as an explicit in-place helper.

If you need the *raw* `<<` syntax and anchor/alias graph identity (not the
expanded result), parse with [`parse_events` / `EventStream`](streaming.md) or
the [lossless graph](editing.md#inspect-without-editing).

## Numbers, timestamps, and binary

`Number` widens to `i128` / `u128`; the usual helpers are range-checked:

```rust
let v: saneyaml::Value = saneyaml::from_str("count: 9000000000000000000\n")?;
assert_eq!(v["count"].as_u64(), Some(9_000_000_000_000_000_000));
```

Timestamps and `!!binary` are YAML 1.1 features and need an explicit schema or
tag. `!!timestamp` scalars are read via `as_timestamp()` / typed
`saneyaml::Timestamp` fields; `!!binary` decodes into byte targets like
`Vec<u8>`. See [Schema modes](schema-modes.md) for enabling YAML 1.1 typing.

## Control emitted YAML

The default (`EmitOptions::structural()`) is insertion-order, plain-where-safe,
block layout. Tune it with builder methods:

```rust
use saneyaml::{EmitOptions, KeyOrder, ScalarQuoteStyle};

let opts = EmitOptions::structural()
    .with_key_order(KeyOrder::Sort)                    // Preserve | Sort
    .with_scalar_quote_style(ScalarQuoteStyle::DoubleQuoted); // PlainWhereSafe | SingleQuoted | DoubleQuoted

let text = saneyaml::to_string_with_options(&cfg, opts)?;
```

Other knobs: `with_collection_style` (`Block` | `Flow`), `with_block_scalar_style`
(`Literal` | `Folded`), `with_enum_representation` (`Tag` | `SingletonMap`), and
`with_yaml_1_1_safe_strings(true)` to quote strings like `no` / `12:34:56` so
older YAML 1.1 readers don't reinterpret them.

For byte-for-byte `serde_yaml` writer output on the supported structural corpus,
use `EmitOptions::byte_compatible()`. Comments and source formatting are not a
writer concern — use [in-place editing](editing.md) to preserve them.

## Custom tags

Application tags (`!Ref`, `!Env`, CloudFormation intrinsics, …) are preserved in
`Value` and visible to enum dispatch. For ordinary typed reads they're
transparent metadata:

```rust
// !Env prod          -> String "prod"
// !Ports [80, 443]   -> Vec<u16>
// !Maybe null        -> Option<T> (None)
```

Inspect a tag explicitly with `value.as_tagged()`, which exposes the tag and the
inner `Value`.
