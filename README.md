# yaml

A production-oriented pure-Rust YAML parser/emitter prototype for common
developer configuration files.

Status: local developer preview. The crate is intentionally not published yet
(`publish = false`), and the public crate name/license still need final release
decisions. Start with [DEVELOPER_PREVIEW.md](DEVELOPER_PREVIEW.md),
[MIGRATION.md](MIGRATION.md), [COMPATIBILITY.md](COMPATIBILITY.md), and
[BASELINE.md](BASELINE.md) before evaluating it in another project.

The first milestone focuses on:

- YAML 1.2 core scalar resolution for parser events and a document tree.
- Ordered mappings, block/flow collections, quoted/plain scalars, and basic
  literal/folded block scalars.
- Acyclic anchors and aliases expanded into the loaded tree.
- Default YAML merge-key expansion for loaded trees and Serde reads, while raw
  parser events still expose `<<` and alias events.
- Deterministic structural emission with `parse(emit(tree)) == tree` for
  emittable trees; duplicate-effective mapping keys, untagged literal merge
  keys, over-depth trees, and directly nested tags are rejected before output.
- Serde read support through `yaml::from_str` and a spanless
  `serde_yaml`-style `yaml::Value`, including source-backed string reads and
  typed `i128`/`u128` integer targets for large config identifiers, plus
  `yaml::to_value`, `yaml::value::to_value`, `yaml::value::Serializer`,
  `yaml::to_string`, `yaml::to_writer`, and `yaml::Serializer<W>` for common
  config-shaped `Serialize` values with `serde_yaml`-style 128-bit integer
  value serialization, tagged values, and document markers.
- A `serde_yaml` swap harness and migration-readiness report for common
  downstream config-loading paths.
- Pinned external replay fixtures from Pingora, rust-i18n, and cfn-guard that
  compare real downstream YAML inputs against `serde_yaml`.
- A downstream-shaped migration harness, compileable migration example,
  Ubuntu-only CI workflow, non-mutating all-target fuzz smoke script, and
  real-world config benchmark command.
- Clear diagnostics with line/column spans.
- Property tests under `cargo test` plus optional `cargo-fuzz` targets.

Intentional first-milestone non-goals:

- YAML 1.1 implicit booleans and timestamps.
- YAML graph identity, comment preservation, lossless formatting, and
  directive-preserving emission.
- Kubernetes schema validation or automated ecosystem migration tooling.

## Verification

```sh
cargo test --test serde_yaml_swap_harness
cargo test --test downstream_migration_harness
cargo test --test external_downstream_migration
cargo test --test libyaml_probe_manifest
cargo test --test baseline_audit
RUSTDOCFLAGS='-D missing_docs' cargo doc --no-deps
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --release --example real_world_benchmark
scripts/fuzz-smoke-nonmutating.sh
```

`tests/baseline_audit.rs` verifies that `BASELINE.md` matches the committed
manifest, registry, migration report, corpus, and command evidence. `cargo
fuzz` is optional for ordinary development; the script copies corpora to a
temporary directory before running all four targets so it does not grow tracked
corpus files. Parser safety properties are also exercised by
`tests/parser_properties.rs`, which runs with plain `cargo test`.
