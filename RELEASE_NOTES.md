# Release Notes

## 0.1.0 release-candidate work in progress

This crate is still a local developer preview and is not published. Current
release blockers are the public crate name, final license, version decision,
and explicit user approval for a crates.io push.

Notable completed release-candidate behavior:

- Phase 0 campaign rails now include a live conformance dashboard test over the
  pinned YAML test-suite denominator. It reports 402 upstream cases, 402
  selected/classified, 0 unselected, 306 accepted, 96 rejected, and keeps
  YAML-suite and Psych/libyaml divergence overlays separate from outcome
  buckets.
- Phase 2 parity-ledger work has closed event parity across the selected YAML
  test-suite denominator while leaving the remaining loaded-tree and
  shared-reference deferrals as explicit policy records.
- The latest parity-policy tranches split the former broad catch-all deferrals
  into named loaded-tree, null-document, flow, stream, anchor-character, and
  indentation families, with named fuzz/span coverage before any loaded-tree or
  Serde policy promotion.
- Emission now has explicit fidelity tiers through `EmitOptions`:
  `Structural` is the implemented default, `ByteCompatible` is opt-in
  `serde_yaml` writer-byte parity for the supported structural corpus, and
  `Preserving` remains a declared future target tier that returns a
  not-implemented error instead of silently falling back to structural output.
- `Cargo.toml` now has an explicit package include boundary for the developer
  preview: source, examples, and public docs are package contents, while
  repository-only fixtures, downstream reductions, fuzz corpora, CI files, and
  proof scripts stay local until the public license/redistribution policy is
  selected.
- Default loaded-tree, `from_value`, direct owned/borrowed `Value`
  deserializers, and Serde reads expand untagged and explicit `!!merge` /
  canonical merge-tag keys while raw parser events retain merge syntax and tag
  metadata.
- Explicit YAML core scalar tags retain tag/source metadata and support typed
  Serde reads for strings, booleans, nulls, integers, floats, timestamps, and
  binary byte targets.
- YAML 1.1 collection and structural tags `!!set`, `!!omap`, `!!pairs`,
  `!!seq`, `!!map`, and `!!value` retain tagged payloads in `Node`/`Value` and
  support typed Serde reads for set-like targets, ordered pair sequences,
  `!!omap` map targets, duplicate-preserving `!!pairs` pair sequences, sequence
  targets, map/struct targets, scalar `!!value` reads, and custom `%TAG`
  handles that resolve to those YAML core tags.
- The migration harness records YAML 1.1 scalar construction as an explicit
  call-site choice, including the default decimal treatment of `0123` versus
  YAML 1.1 octal interpretation under `LoadOptions`.
- YAML 1.1 loading now recovers Psych/libyaml merge edges for repeated real
  merge keys and non-mergeable merge payloads while default YAML 1.2-oriented
  loading keeps those cases strict.
- A checked-in strict package-alias smoke fixture executes upstream-compatible
  `serde_yaml::...` paths against both `serde_yaml 0.9.34` and this package
  through `serde_yaml = { package = "yaml", ... }`; the expanded alias smoke
  separately covers root document-stream helpers, explicit YAML 1.1
  `LoadOptions`, bounded large-reader behavior, caller-built default merge
  deserialization plus explicit in-place merge expansion, mapping/index
  ergonomics, lossless graph identity inspection, and diagnostic locations. A separate
  real-world package-alias smoke now copies the fixture registry into a clean
  downstream crate, parses every registered fixture through
  `serde_yaml::Deserializer`, and keeps representative deep field assertions for
  GitHub Actions, Docker Compose, Kubernetes, Helm, OpenAPI, Wrangler, and
  Ansible documents through `serde_yaml::...` imports, including CRD schemas,
  Helm values/dependencies, OpenAPI polymorphism, Wrangler durable object
  migrations, and Ansible vault/unsafe tags.
- Pinned external downstream replay and package-alias build trials now include
  Pingora, rust-i18n, cfn-guard, navi, Stackable operator-rs, figment, and
  uaparser. The package smoke replays checked-in reductions from the fixture
  downstreams through the packaged crate under the `serde_yaml` dependency
  name, covering typed Pingora config reads, locale trees, CloudFormation
  short-form tags, navi string and reader config loads, and Stackable
  CRD/OpenAPI shapes before the live checkout trials run. Stackable adds
  Kubernetes CRD/OpenAPI schema fixture coverage plus serializer and scalar
  serde build checks; figment adds an optional table-style dependency and YAML
  provider test, while uaparser adds a large bundled `regexes.yaml` corpus
  through slice, reader, and example parser paths.
- External downstream writer replay now covers cfn-guard tagged
  CloudFormation `Value` fixtures and every checked-in Stackable CRD through
  `to_string`, `to_writer`, and streaming `Serializer`, both in direct tests
  and in the packaged `serde_yaml` alias smoke.
- YAML 1.1 conformance fixtures now cover directive-driven legacy scalar
  construction, bool/null aliases, null and float spellings, radix and
  sexagesimal numerics, oversized numeric spellings, timestamp time-zone and
  leap-second forms, explicit binary whitespace, collection and structural tags,
  invalid binary typed-target diagnostics, flow-style scalar collections and
  mapping keys, merge-key expansion under legacy schema selection, malformed
  collection-tag typed-read diagnostics, and boolean, numeric, signed-zero, and
  alias-expanded duplicate-key diagnostics.
- `parse_lossless` / `LosslessStream` preserve comments, trivia, anchors,
  aliases, and stable graph ids for source-backed inspection, replay, and
  validated node/source-span edits, insertions, and deletions that preserve
  untouched bytes.
- `LosslessStream::effective_mapping_entries` adds a read-only view over
  mapping nodes that expands merge aliases for inspection while preserving raw
  `<<` nodes, original source text, and alias/anchor provenance. The effective
  mapping tests cover overridden merge entries and the real-world Compose
  fragment proves effective environment keys without rewriting the source.
- Real-world lossless replay is now manifest-gated for GitHub Actions workflow
  comments, flow-style branch/matrix lists, Docker Compose healthcheck flow
  commands, Helm chart comments, OpenAPI block scalars/flow collections,
  Wrangler comments/flow flags, Ansible `!vault` / `!unsafe` tags, and
  Kubernetes Helm-style streams and ConfigMap block scalars.
- Real-world graph coverage now includes an adapted official Compose
  Specification fragments example with multiple anchors, aliases, and a merge
  list, plus a manifest audit that detects graph-sensitive fixtures.
- Real-world Docker Compose merge-anchor fixtures now carry loaded-tree parity:
  reference-loader trees are normalized through this crate's default merge
  expansion policy, while raw event and lossless graph coverage keep the source
  `<<` syntax visible.
- Lossless source edits and graph identity now have fuzz-corpus replay covering
  scalar, flow mapping, flow sequence replacements, YAML 1.1 merge/comment
  streams, anchor redefinition, and recursive aliases. Anchor/alias target
  identity is also checked against `yaml-rust2` and `saphyr` parser anchor
  events for redefinition, recursive, document-reset, merge, manifest-derived
  selected YAML-suite anchor/alias cases, manifest-owned Docker Compose anchor
  cases, and validated source edits after reparsing. YAML 1.1 merge/comment
  graph seeds are promoted into deterministic conformance fixtures.
- Lossless fuzz/property replay now includes real-world-shaped GitHub Actions,
  Helm, Kubernetes ConfigMap, and OpenAPI seeds that combine comments,
  directives, document markers, block scalars, anchors, aliases, merge keys,
  flow collections, and structural source edits.
- Serde and schema-mode fuzz corpora now require YAML 1.1 malformed `!!set`,
  `!!omap`, and `!!pairs` seeds plus resolved and duplicate `!!value` key
  seeds, so recent compatibility diagnostics replay through the fuzz/property
  safety gates.
- Emitter round-trip fuzz coverage now exercises `to_string`, `to_writer`, and
  streaming `Serializer` output against parsed-tree and `Value` replay.
- A dedicated Serde serializer fuzz target now cross-checks `to_value`,
  `to_string`, `to_writer`, and streaming `Serializer` output for
  struct/map/enum/helper shapes, Kubernetes/OpenAPI-like documents, nested
  options, multi-document streams, and unsupported byte serialization.
- The Ubuntu CI workflow now installs nightly plus `cargo-fuzz` and runs a
  one-pass all-target non-mutating fuzz smoke over copied corpora.
- Direct Serde `IgnoredAny` entrypoints now validate malformed YAML and
  single-document boundaries before skipping a document.
- Divergence records now require caller-facing `migration_impact` text, so
  compatibility decisions are tied to adoption risk instead of only parser
  policy. The pinned Psych/libyaml probe records merge-list precedence,
  explicit merge-tag expansion, explicit merge overrides, repeated merge-key
  recovery, non-mergeable merge payload recovery, fixture-backed YAML 1.1
  merge/tag/graph cross-checks, resolved `!!value` handle and duplicate-key
  policy checks, nested merge precedence, duplicate local-key
  policy, cross-document merge alias reset, mixed invalid merge-list recovery,
  signed-zero and alias-expanded key-collision policy, alias object identity as
  an explicit `LosslessStream` contract split, per-case input digests, error
  locations where Psych exposes them, and first-class alias
  redefinition/recursive identity probes as deliberate compatibility decisions.
- A Psych/libyaml coverage ledger now groups the 49 pinned probe cases into
  eight behavior families. The current probe matrix now has no open tracked
  next-probe gaps, keeping YAML 1.1/libyaml scope auditable without claiming
  blanket compatibility.
- `TaggedValue` now implements owned and borrowed Serde deserializer support
  for direct enum and `IgnoredAny` reads, matching the package-alias
  `serde_yaml::value::TaggedValue` surface.
- `LoadOptions` now applies a default 64 MiB input byte ceiling across parser,
  Serde, reader, document-stream, and direct deserializer entrypoints, and
  `parse_lossless_bytes` now checks that default ceiling before UTF-8 validation.
  Loader paths keep `max_input_bytes()` for byte-limit tuning,
  `max_alias_expansion_nodes()` for alias expansion work tuning, and
  `without_input_limit()` for explicitly pre-bounded sources.
- `LosslessEdit` now has source-backed scalar-keyed block/flow mapping entry
  helpers for value replacement, entry insertion, and entry deletion while
  preserving untouched comments, aliases, and formatting and reparsing the
  final YAML.
- `LosslessEdit` now also has source-backed block/flow sequence item helpers for
  item replacement, insertion, and deletion, with source-preserving formatting
  plus direct lossless-edit fuzz/property replay seeds.
- The YAML test-suite coverage ledger now pins the full upstream denominator at
  402 cases, maps the 402 selected cases to canonical upstream IDs, and records
  the 0 not-imported cases as explicit coverage debt. The selected set now
  includes additional official graph/property cases covering block and flow
  anchors, aliases, anchored mapping keys, colon-bearing anchor names, node
  property indicators, comments near anchors, and missing explicit mapping
  values, plus tab/exotic-indentation cases covering quoted tab content,
  inline tabs, block scalar tab content, trailing tabs, and invalid tab
  placement, plus the remaining upstream `error` fixtures as rejected-with-policy
  conformance cases.
- Fuzz release proof now has a manual `scripts/fuzz-release-sweep.sh` path that
  records checkout HEAD/status, target mode, target names, corpus counts, run
  counts, statuses, elapsed time, and artifact directories, while
  `parser_properties` gates corpus target parity, release floors, and named
  safety seeds.

Known release-candidate gaps remain tracked in `BASELINE.md`,
`COMPATIBILITY.md`, and `MIGRATION.md`: complete YAML 1.1 ecosystem parity,
arbitrary `serde_yaml` byte parity outside the supported `ByteCompatible`
structural writer corpus, full arbitrary structural lossless formatting beyond
targeted block/flow mapping entry and sequence item helpers,
`EmitOptions::Preserving`, final package metadata, and external publication.
Semantic `Node`/`Value` loaders remain value-oriented by design; alias graph
identity is the `LosslessStream` contract.
