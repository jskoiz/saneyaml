# Diagnostics

Errors carry where and what — line/column, an in-document key path, and an
opt-in source caret — so a bad config tells the user how to fix it.

> Snippets elide the enclosing function; assume a function returning
> `saneyaml::Result<()>`.

## Location

`Error::line()` and `column()` mirror the common `serde_yaml` convenience path
(`location()` returns the same as a `Location`):

```rust
let err = saneyaml::from_str::<Config>("name: [\n").unwrap_err();

if let (Some(line), Some(col)) = (err.line(), err.column()) {
    eprintln!("error at {line}:{col}");
}
```

## Key path

`path()` reports where in the document the error is, using familiar traversal
syntax — `server.port`, `ports[1]`, bracket-quoted non-identifier keys:

```rust
let err = saneyaml::from_str::<Config>("server:\n  port: not-a-number\n").unwrap_err();

if let Some(path) = err.path() {
    eprintln!("at {path}"); // at server.port
}
```

## Source caret

`render_source(input)` returns a `Display` that points at the offending span,
rustc-style — great for CLI output:

```rust
let input = "name: web\nport: [\n";
let err = saneyaml::from_str::<Config>(input).unwrap_err();

eprintln!("{}", err.render_source(input));
// 2 | port: [
//   |       ^ …
```

Use `render_source_with_options(input, SourceRenderOptions)` to control the
number of context lines.

## Categorize

`category()` returns an `ErrorCategory` for branching — e.g. distinguishing a
parse/syntax failure from a type mismatch. `document_index()` reports which
document in a stream failed (zero-based).

```rust
use saneyaml::ErrorCategory;

match err.category() {
    ErrorCategory::Syntax => { /* malformed YAML */ }
    ErrorCategory::Data   => { /* type/shape mismatch */ }
    _ => { /* Limit, Reference, DuplicateKey, Io, … */ }
}
```

## What carries spans

| Source | Line/column? |
|---|---|
| `from_str` / `from_slice` / `from_node` | yes |
| `Deserializer::from_str` / `from_slice` (incl. stream items) | yes |
| `from_reader` (after buffering) | yes |
| `from_value` and direct `Value` reads | no — `Value` is spanless |

The flat `Display` string stays compatible with `serde_yaml`; everything above
is additive.
