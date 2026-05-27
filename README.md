# yaml

A production-oriented pure-Rust YAML parser/emitter prototype for common
developer configuration files.

The first milestone focuses on:

- YAML 1.2 core scalar resolution for parser events and a document tree.
- Ordered mappings, block/flow collections, quoted/plain scalars, and basic
  literal/folded block scalars.
- Acyclic anchors and aliases expanded into the loaded tree.
- Deterministic structural emission with `parse(emit(tree)) == tree` for
  emittable trees; duplicate-effective mapping keys, over-depth trees, and
  directly nested tags are rejected before output.
- Serde read support through `yaml::from_str` and a spanless
  `serde_yaml`-style `yaml::Value`, including source-backed string reads and
  typed `i128`/`u128` integer targets for large config identifiers, plus
  `yaml::to_value`, `yaml::value::to_value`, `yaml::value::Serializer`,
  `yaml::to_string`, `yaml::to_writer`, and `yaml::Serializer<W>` for common
  config-shaped `Serialize` values with `serde_yaml`-style 128-bit integer
  value serialization, tagged values, and document markers.
- Clear diagnostics with line/column spans.
- Property tests under `cargo test` plus an optional `cargo-fuzz` target.

Intentional first-milestone non-goals:

- YAML 1.1 implicit booleans and timestamps.
- YAML graph identity, YAML 1.1 merge-key expansion, comment preservation,
  lossless formatting, and directive-preserving emission.
- Kubernetes schema validation or ecosystem migration tooling.

## Verification

```sh
cargo test --test baseline_audit
cargo test
cargo clippy --all-targets -- -D warnings
PATH=/Users/jk/.rustup/toolchains/nightly-aarch64-apple-darwin/bin:$PATH cargo fuzz run parse_bytes -- -runs=1000
```

`tests/baseline_audit.rs` verifies that `BASELINE.md` matches the committed
manifest, registry, corpus, and command evidence. `cargo fuzz` is optional for
ordinary development. Parser safety properties are also exercised by
`tests/parser_properties.rs`, which runs with plain `cargo test`.
