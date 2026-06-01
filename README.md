# yaml

A production-oriented pure-Rust YAML parser/emitter prototype for common
developer configuration files.

Status: local developer preview. The crate is intentionally not published yet
(`publish = false`), and the public crate name/license still need final release
decisions. Start with [DEVELOPER_PREVIEW.md](DEVELOPER_PREVIEW.md),
[MIGRATION.md](MIGRATION.md), [COMPATIBILITY.md](COMPATIBILITY.md), and
[BASELINE.md](BASELINE.md) before evaluating it in another project.
The Cargo package include list is explicit: it packages source, examples, and
public documentation, while repository-only fixtures, fuzz corpora, CI files,
and proof scripts remain local evidence until the final redistribution policy is
chosen.

The first milestone focuses on:

- Raw parser events preserve scalar values, style, tags, anchors, and directive
  metadata; constructed document trees use YAML 1.2 core scalar resolution by
  default.
- Explicit schema modes through `LoadOptions::{core, json, failsafe,
  legacy_serde_yaml}` and `Schema::{Core, Json, Failsafe,
  LegacySerdeYaml}`. Default loading stays YAML 1.2-oriented through the
  retained `Schema::Yaml12` spelling; `Schema::Yaml11` remains the retained
  spelling for the legacy mode. Legacy construction covers YAML 1.1
  booleans/nulls, `yaml::Timestamp` typed reads for timestamp-shaped scalars,
  and numeric forms that fit the current value model, plus
  `LoadOptions::yaml_version_directive()` for callers that want `%YAML 1.1`
  document headers to select that legacy construction mode. Explicit YAML core
  tags are recognized in both short `!!int` and canonical
  `tag:yaml.org,2002:int` forms, including typed Serde reads for YAML 1.1
  `!!set`, `!!omap`, `!!pairs`, `!!seq`, `!!map`, and `!!value` tags while
  retaining the tags in `Node`/`Value`; custom `%TAG` handles that resolve to
  those YAML core tags are covered too. Directive-driven fixtures cover
  flow-style scalar collections and keys, and default loading keeps the YAML
  1.2-oriented decimal treatment of leading-zero values unless YAML 1.1 mode is
  selected.
- Ordered mappings, block/flow collections, quoted/plain scalars, and basic
  literal/folded block scalars.
- Acyclic anchors and aliases expanded into the loaded tree.
- Default YAML merge-key expansion for loaded trees, `from_value`, and direct
  owned/borrowed `Value` Serde reads, including explicit `!!merge` / canonical
  merge-tag keys, while raw parser events still expose `<<`, key tags, and
  alias events. Real-world Docker Compose merge-anchor fixtures are
  loaded-tree gated by comparing merge-expanded reference-loader trees against
  this crate's default-expanded output.
- Pull-based `yaml::EventStream` and `yaml::DocumentStream` APIs, available
  through root `stream_events*` / `stream_documents*` helpers and
  `LoadOptions`, consume parser events or completed documents without
  retaining the full event vector or document graph. Event streaming is the raw
  non-expanding contract and matches `parse_events` event-for-event on
  successful input; document streaming yields merge-expanded `Node` documents
  one at a time and matches `parse_documents`. Input is still fully buffered;
  streaming bounds the retained parsed representation, not source bytes. Reader
  constructors use the same bounded reader ingestion as `from_reader` before
  yielding the pull iterator.
- A source-backed `yaml::parse_lossless` / `yaml::LosslessStream` API that
  keeps the original source for byte-stable replay, exposes comments and blank
  lines as trivia, represents anchors/aliases with stable graph ids, compares
  those ids against `yaml-rust2` and `saphyr` parser anchor events for
  manifest-owned selected YAML-suite anchor/alias cases, real-world Compose
  graph fixtures, YAML 1.1 graph fixtures, and edited output after reparsing,
  exposes a read-only effective mapping view that expands merge aliases while
  retaining raw `<<` source and alias/anchor provenance, and can produce
  validated node and raw source-span edits, block/flow mapping entry
  value/insert/delete edits, block/flow sequence item value/insert/delete
  edits, and raw insertions/deletions
  while preserving untouched bytes. Real-world Docker Compose anchor cases
  including an adapted official Compose-spec fragment, YAML 1.1 merge/comment
  graph fixtures, GitHub Actions workflow comments/flow lists, Docker Compose
  comments/flow healthchecks, Helm chart comments, OpenAPI block scalars/flow
  collections, Wrangler comments/flow flags, and Ansible/Kubernetes lossless
  replay cases are manifest-gated on this surface.
- Deterministic structural emission with `parse(emit(tree)) == tree` for
  emittable trees through the default `EmitOptions::Structural` tier;
  duplicate-effective mapping keys, untagged literal merge keys, over-depth
  trees, and directly nested tags are rejected before output.
  `EmitOptions::ByteCompatible` is opt-in and matches `serde_yaml` bytes for a
  supported structural writer corpus covering common scalars, maps, sequences,
  Serde enum tags, document markers, typed real-world config shapes, and
  bytes rejection. `EmitOptions::Preserving` remains a declared future tier and
  returns an explicit not-implemented error instead of falling back silently.
- Serde read support through `yaml::from_str` and a spanless
  `serde_yaml`-style `yaml::Value`, including source-backed string reads and
  typed `i128`/`u128` integer targets for large config identifiers, plus
  `yaml::to_value`, `yaml::value::to_value`, `yaml::value::Serializer`,
  `yaml::to_string`, `yaml::to_writer`, and `yaml::Serializer<W>` for common
  config-shaped `Serialize` values with `serde_yaml`-style 128-bit integer
  value serialization, tagged values, and document markers.
- Bounded input and alias expansion by default: `LoadOptions` carries a 64 MiB
  input byte ceiling, input-derived alias budget, configurable nesting, scalar,
  and collection limits, and explicit opt-outs for callers that have already
  bounded their source. Raw event streaming validates alias references without
  expanding them; loaded trees, Serde reads, and `DocumentStream` enforce alias
  expansion budgets. `COMPATIBILITY.md` documents the threat model and resource
  guarantees.
- A `serde_yaml` swap harness and migration-readiness report for common
  downstream config-loading paths.
- Pinned external replay fixtures from Pingora, rust-i18n, cfn-guard, navi,
  and Stackable operator-rs that compare real downstream YAML inputs against
  `serde_yaml`, including CLI configs and Kubernetes CRD/OpenAPI schema
  documents.
- Packaged downstream smoke, Pingora, rust-i18n, cfn-guard, navi, Stackable
  operator-rs, figment, and uaparser build trials that consume this crate under
  the `serde_yaml` dependency name from clean temporary checkouts, including a
  strict checked-in smoke fixture that runs upstream-compatible
  `serde_yaml::...` API paths against both `serde_yaml 0.9.34` and this
  package, including exact `with::singleton_map` helper shape behavior, plus an
  expanded package-alias smoke for explicit `LoadOptions`, bounded large-reader
  behavior, pull event/document streaming, merge, mapping/index, lossless
  graph, and diagnostic-location paths, plus a
  packaged real-world alias smoke that copies the fixture registry into a clean
  downstream crate and parses GitHub Actions, Docker Compose, Kubernetes, Helm,
  OpenAPI, Wrangler, and Ansible files through `serde_yaml::...` imports, plus
  an external downstream package-alias smoke over checked-in Pingora,
  rust-i18n, cfn-guard, navi, and Stackable operator fixtures, including
  tagged CloudFormation and Stackable CRD writer replay through `to_string`,
  `to_writer`, and streaming `Serializer`. The figment trial covers an optional
  table-style `serde_yaml` dependency plus YAML provider tests; the uaparser
  trial covers large bundled `regexes.yaml` data through slice, reader, and
  example parser paths.
- A downstream-shaped migration harness, compileable migration example,
  Ubuntu-only CI workflow with all-target fuzz-smoke wiring, non-mutating
  fuzz replay script, and real-world config benchmark command.
- Clear diagnostics with line/column spans. The default `Display` remains the
  compact `message at line L, column C` string, while `Error` also exposes a
  broad `ErrorCategory`, optional in-document `ErrorPath`, optional
  zero-based document index for stream failures, and explicit
  `render_source(...)` caret rendering over caller-provided source text.
- A live conformance dashboard test over the pinned YAML test-suite denominator:
  402 selected/classified cases out of 402, with 0 unselected cases tracked
  as coverage debt and documented divergence overlays kept separate from
  accepted/rejected outcome counts.
- Property tests under `cargo test` plus optional `cargo-fuzz` targets.

Intentional first-milestone non-goals:

- Full YAML 1.1 compatibility: collection/structural tags, explicit scalar
  tags, directive-driven scalar edges, fixture-backed Psych-style merge-edge
  recovery, directive stream-boundary behavior, YAML 1.1 lossless graph
  parser-event cross-checks, and an eight-family Psych/libyaml coverage ledger
  with no open tracked next-probe gaps are covered, but broader libyaml-era
  behavior and schema/API completeness decisions still remain.
- Full arbitrary structural lossless editing beyond targeted block/flow mapping
  entry and sequence item helpers and directive-preserving structural emission.
  Semantic `Node`/`Value` loaders intentionally stay value-oriented; use
  `LosslessStream` for alias graph identity and merge-effective source
  inspection.
- Full reference-runtime parity across the upstream YAML test-suite is not
  claimed yet; the pinned coverage ledger records 402 upstream cases, 402
  selected cases, and 0 not-imported cases as explicit coverage debt. The
  dashboard is the current progress counter; Phase 1 parser-denominator
  classification is closed.
- Arbitrary `serde_yaml` byte parity for source-styled trees, comments,
  anchors/aliases, directives, and lossless formatting remains outside the
  first `EmitOptions::ByteCompatible` corpus; use `LosslessStream` for
  source-preserving replay. `EmitOptions::Preserving` is still a reserved
  future tier and selecting it currently returns an error.
- Kubernetes schema validation or automated ecosystem migration tooling.

## Verification

```sh
cargo test --test serde_yaml_swap_harness
cargo test --test downstream_migration_harness
cargo test --test external_downstream_migration
cargo test --test libyaml_probe_manifest
cargo test --test yaml11_conformance
cargo test --test yaml_suite_coverage
cargo test --test conformance_dashboard -- --nocapture
cargo test --test lossless_roundtrip --test graph_identity --test real_world_lossless
scripts/downstream-build-trials.sh smoke-only
scripts/downstream-build-trials.sh pingora
scripts/downstream-build-trials.sh rust-i18n
scripts/downstream-build-trials.sh cfn-guard
scripts/downstream-build-trials.sh navi
scripts/downstream-build-trials.sh stackable-operator
scripts/downstream-build-trials.sh figment
scripts/downstream-build-trials.sh uaparser
cargo test --test baseline_audit
RUSTDOCFLAGS='-D missing_docs' cargo doc --no-deps
cargo test
cargo clippy --all-targets -- -D warnings
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
cargo run --release --example real_world_benchmark
scripts/fuzz-smoke-nonmutating.sh
scripts/fuzz-release-sweep.sh
```

`tests/baseline_audit.rs` verifies that `BASELINE.md` matches the committed
manifest, registry, migration report, package boundary, corpus, and command evidence. `cargo
fuzz` is optional for ordinary development; the script copies corpora to a
temporary directory before running all ten targets so it does not grow tracked
corpus files. CI runs that script with one requested pass per target to verify
the wiring. `scripts/fuzz-release-sweep.sh` is the manual release gate: it runs
the same ten targets with a configurable budget and writes a summary with
checkout HEAD/status, target mode, target names, corpus counts, run counts,
statuses, elapsed time, and artifact directories. Unfiltered release sweeps must
cover every target declared in `fuzz/Cargo.toml`. Sustained fuzzing and minimized findings remain separate
release-readiness activity.
Parser safety properties are also exercised by
`tests/parser_properties.rs`, which runs with plain `cargo test`.
