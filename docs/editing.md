# Editing files

Change values in an existing YAML file while keeping every comment, anchor,
blank line, and untouched byte exactly where it was. This is the part a
load-then-re-emit round trip can't do.

> Snippets elide the enclosing function; assume a function returning
> `saneyaml::Result<()>`.

## Edit by path

`saneyaml::edit` opens a `ConfigEditor`. Address values by path, then `finish`:

```rust
let source = "\
# service stack
services:
  web:
    image: nginx:1.25
    ports:
      - \"80:80\"
";

let mut editor = saneyaml::edit(source)?;
editor
    .set(saneyaml::ConfigPath::keys(["services", "web", "image"]), "nginx:1.27")?
    .push(saneyaml::ConfigPath::keys(["services", "web", "ports"]), "8080:80")?;

let edited = editor.finish()?;
assert!(edited.contains("# service stack"));   // comment preserved
assert!(edited.contains("image: nginx:1.27")); // value updated
assert!(edited.contains("- 8080:80"));         // item appended
```

Operations: `set`, `insert`, `remove`, `rename`, `push` (append to a sequence),
and `insert_item` (insert at an index). Each returns `&mut Self`, so chain them;
the editor reparses between operations so later paths see current source.

## Addressing paths

```rust
use saneyaml::{ConfigPath, PathSegment};

// string keys (most common)
ConfigPath::keys(["metadata", "labels", "app"]);

// mixed keys and sequence indices
ConfigPath::new([
    PathSegment::from("jobs"),
    PathSegment::from("test"),
    PathSegment::from("steps"),
    PathSegment::from(0usize),
    PathSegment::from("uses"),
]);

// JSON Pointer — handles keys containing "/" or "~"
ConfigPath::json_pointer("/metadata/labels/app.kubernetes.io~1name")?;
```

## Read and write files directly

```rust
let mut editor = saneyaml::edit_file("compose.yaml")?;
editor.set(saneyaml::ConfigPath::keys(["version"]), "3.9")?;
editor.finish_to_file()?; // writes back to compose.yaml
```

## Inspect without editing

Drop to `LosslessStream` when you need to *read* source-level detail — comments,
exact scalar spelling, anchor/alias graph identity — that the semantic `Value`
tree discards:

```rust
let stream = saneyaml::parse_lossless(source)?;

for comment in stream.comments() {
    println!("{}", comment.text());
}
```

`LosslessStream` also exposes `effective_mapping_entries(node)` — the merged view
of a mapping *with* `<<` provenance kept — and `source_fragment(span)` to recover
the original bytes for any node. It's the surface for tools that must preserve or
analyze source, not just values.

A runnable end-to-end example (Docker Compose, Kubernetes, GitHub Actions) lives
in [`examples/config_refactor.rs`](../examples/config_refactor.rs).
