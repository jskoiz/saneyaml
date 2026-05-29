# yaml

A production-oriented pure-Rust YAML parser/emitter prototype for common
developer configuration files.

Status: local developer preview. The crate is intentionally not published yet
(`publish = false`), and the public crate name/license still need final release
decisions. Start with [DEVELOPER_PREVIEW.md](DEVELOPER_PREVIEW.md),
[MIGRATION.md](MIGRATION.md), [COMPATIBILITY.md](COMPATIBILITY.md), and
[BASELINE.md](BASELINE.md) before evaluating it in another project.

The first milestone focuses on:

- Raw parser events preserve scalar text, style, tags, anchors, and directive
  metadata; constructed document trees use YAML 1.2 core scalar resolution by
  default.
- Explicit `LoadOptions::yaml_1_1()` construction for legacy YAML 1.1
  booleans/nulls, `yaml::Timestamp` typed reads for timestamp-shaped scalars,
  and numeric forms that fit the current value model, plus
  `LoadOptions::yaml_version_directive()` for callers that want `%YAML 1.1`
  document headers to select that legacy construction mode. Explicit YAML core
  tags are recognized in both short `!!int` and canonical
  `tag:yaml.org,2002:int` forms, including typed Serde reads for YAML 1.1
  `!!set`, `!!omap`, `!!pairs`, `!!seq`, `!!map`, and `!!value` tags while
  retaining the tags in `Node`/`Value`; directive-driven fixtures cover
  flow-style scalar collections and keys, and default loading keeps the YAML
  1.2-oriented decimal treatment of leading-zero values unless YAML 1.1 mode is
  selected.
- Ordered mappings, block/flow collections, quoted/plain scalars, and basic
  literal/folded block scalars.
- Acyclic anchors and aliases expanded into the loaded tree.
- Default YAML merge-key expansion for loaded trees and Serde reads, including
  explicit `!!merge` / canonical merge-tag keys, while raw parser events still
  expose `<<`, key tags, and alias events.
- A source-backed `yaml::parse_lossless` / `yaml::LosslessStream` API that
  keeps the original source for byte-stable replay, exposes comments and blank
  lines as trivia, represents anchors/aliases with stable graph ids, compares
  those ids against `yaml-rust2` and `saphyr` parser anchor events for
  manifest-owned selected YAML-suite anchor/alias cases, real-world Compose
  graph fixtures, and YAML 1.1 graph fixtures, and can produce validated node
  and raw source-span edits, insertions, and deletions while preserving
  untouched bytes. Real-world Docker Compose anchor cases including an adapted
  official Compose-spec fragment, YAML 1.1 merge/comment graph fixtures, and
  Ansible/Kubernetes lossless replay cases are manifest-gated on this surface.
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
- Pinned external replay fixtures from Pingora, rust-i18n, cfn-guard, and
  Stackable operator-rs that compare real downstream YAML inputs against
  `serde_yaml`, including Kubernetes CRD/OpenAPI schema documents.
- Packaged downstream smoke, Pingora, rust-i18n, cfn-guard, and Stackable
  operator-rs build trials that consume this crate under the `serde_yaml`
  dependency name from clean temporary checkouts, including a strict checked-in
  smoke fixture that runs upstream-compatible `serde_yaml::...` API paths
  against both `serde_yaml 0.9.34` and this package, plus an expanded
  package-alias smoke for explicit `LoadOptions`, document-stream, merge,
  mapping/index, lossless graph, and diagnostic-location paths, plus a
  packaged real-world alias smoke that copies the fixture registry into a clean
  downstream crate and parses GitHub Actions, Docker Compose, Kubernetes, Helm,
  OpenAPI, Wrangler, and Ansible files through `serde_yaml::...` imports, plus
  an external downstream package-alias smoke over checked-in Pingora,
  rust-i18n, cfn-guard, and Stackable operator fixtures.
- A downstream-shaped migration harness, compileable migration example,
  Ubuntu-only CI workflow with all-target fuzz-smoke wiring, non-mutating
  fuzz replay script, and real-world config benchmark command.
- Clear diagnostics with line/column spans.
- Property tests under `cargo test` plus optional `cargo-fuzz` targets.

Intentional first-milestone non-goals:

- Full YAML 1.1 compatibility: collection/structural tags, explicit scalar
  tags, directive-driven scalar edges, and Psych-style merge-edge recovery are
  covered, but broader libyaml-era behavior and schema/API completeness
  decisions still remain.
- Full structural lossless editing beyond validated source-span editing,
  directive-preserving structural emission, and graph identity in the semantic
  `Node`/`Value` loaders.
- Kubernetes schema validation or automated ecosystem migration tooling.

## Verification

```sh
cargo test --test serde_yaml_swap_harness
cargo test --test downstream_migration_harness
cargo test --test external_downstream_migration
cargo test --test libyaml_probe_manifest
cargo test --test yaml11_conformance
cargo test --test lossless_roundtrip --test graph_identity --test real_world_lossless
scripts/downstream-build-trials.sh smoke-only
scripts/downstream-build-trials.sh pingora
scripts/downstream-build-trials.sh rust-i18n
scripts/downstream-build-trials.sh cfn-guard
scripts/downstream-build-trials.sh stackable-operator
cargo test --test baseline_audit
RUSTDOCFLAGS='-D missing_docs' cargo doc --no-deps
cargo test
cargo clippy --all-targets -- -D warnings
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
cargo run --release --example real_world_benchmark
scripts/fuzz-smoke-nonmutating.sh
```

`tests/baseline_audit.rs` verifies that `BASELINE.md` matches the committed
manifest, registry, migration report, corpus, and command evidence. `cargo
fuzz` is optional for ordinary development; the script copies corpora to a
temporary directory before running all eight targets so it does not grow tracked
corpus files. CI runs that script with one requested pass per target to verify
the wiring; sustained fuzzing remains a separate release-readiness activity.
Parser safety properties are also exercised by
`tests/parser_properties.rs`, which runs with plain `cargo test`.
