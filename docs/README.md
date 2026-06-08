# saneyaml documentation

Serde-first YAML for Rust, with real YAML 1.2 semantics. Read this as a hosted
guide at <https://jskoiz.github.io/saneyaml/>, or start with **Getting started**
in the repo and open a topic page when you hit it.

### Learn the basics

- **[Getting started](getting-started.md)** — install, parse into a struct,
  emit. ~5 minutes.
- **[Cookbook](cookbook.md)** — copy-paste recipes for the common tasks.

### Topic guides

- **[Schema modes](schema-modes.md)** — YAML 1.2 vs 1.1, and why `NO` stays the
  string `"NO"`.
- **[Diagnostics](diagnostics.md)** — line/column, key paths, and source carets
  in errors.
- **[Untrusted input](untrusted-input.md)** — resource limits for hostile YAML.
- **[Editing files](editing.md)** — change values in place without losing
  comments, anchors, or ordering.
- **[Streaming](streaming.md)** — pull events/documents with bounded memory.

### Migrating

- **[From serde_yaml](MIGRATION.md)** — drop-in alias and a call-site cookbook.

### Reference

- **[Compatibility](COMPATIBILITY.md)** — scalar resolution table, divergences,
  threat model.
- **[Architecture](ARCHITECTURE.md)** — crate layout and design decisions.
- **[Benchmarks](BENCHMARKS.md)** — throughput and memory vs other crates.
- **[API reference (docs.rs)](https://docs.rs/saneyaml)** — full generated docs.

### Project

[Changelog](../CHANGELOG.md) · [Security policy](../SECURITY.md) ·
[Contributing](../CONTRIBUTING.md) · [License](../LICENSE.md)
