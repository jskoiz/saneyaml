# Release Notes

## 0.1.0 release-candidate work in progress

This crate is still a local developer preview and is not published. Current
release blockers are the public crate name, final license, version decision,
and explicit user approval for a crates.io push.

Notable completed release-candidate behavior:

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
  targets, map/struct targets, and scalar `!!value` reads.
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
  `LoadOptions`, caller-built default merge deserialization plus explicit
  in-place merge expansion, mapping/index ergonomics, lossless graph identity
  inspection, and diagnostic locations. A separate
  real-world package-alias smoke now copies the fixture registry into a clean
  downstream crate, parses every registered fixture through
  `serde_yaml::Deserializer`, and keeps representative deep field assertions for
  GitHub Actions, Docker Compose, Kubernetes, Helm, OpenAPI, Wrangler, and
  Ansible documents through `serde_yaml::...` imports, including CRD schemas,
  Helm values/dependencies, OpenAPI polymorphism, Wrangler durable object
  migrations, and Ansible vault/unsafe tags.
- Pinned external downstream replay and package-alias build trials now include
  Pingora, rust-i18n, cfn-guard, and Stackable operator-rs. The package smoke
  now also replays checked-in reductions from those downstreams through the
  packaged crate under the `serde_yaml` dependency name, covering typed Pingora
  config reads, locale trees, CloudFormation short-form tags, and Stackable
  CRD/OpenAPI shapes before the live checkout trials run. Stackable adds
  Kubernetes CRD/OpenAPI schema fixture coverage plus serializer and scalar
  serde build checks.
- YAML 1.1 conformance fixtures now cover directive-driven legacy scalar
  construction, null and float spellings, timestamp time-zone forms, explicit
  binary, collection, and structural tags, invalid binary typed-target diagnostics,
  flow-style scalar collections and mapping keys, merge-key expansion under
  legacy schema selection, malformed collection-tag typed-read diagnostics, and
  boolean/numeric-key duplicate diagnostics.
- `parse_lossless` / `LosslessStream` preserve comments, trivia, anchors,
  aliases, and stable graph ids for source-backed inspection, replay, and
  validated node/source-span edits, insertions, and deletions that preserve
  untouched bytes.
- Real-world lossless replay is now manifest-gated for Ansible `!vault` /
  `!unsafe` tags and Kubernetes Helm-style streams and ConfigMap block scalars.
- Real-world graph coverage now includes an adapted official Compose
  Specification fragments example with multiple anchors, aliases, and a merge
  list, plus a manifest audit that detects graph-sensitive fixtures.
- Lossless source edits and graph identity now have fuzz-corpus replay covering
  scalar, flow mapping, flow sequence replacements, YAML 1.1 merge/comment
  streams, anchor redefinition, and recursive aliases. Anchor/alias target
  identity is also checked against `yaml-rust2` and `saphyr` parser anchor
  events for redefinition, recursive, document-reset, merge, manifest-derived
  selected YAML-suite anchor/alias cases, and manifest-owned Docker Compose
  anchor cases. YAML 1.1 merge/comment graph seeds are promoted into
  deterministic conformance fixtures.
- Serde and schema-mode fuzz corpora now require YAML 1.1 malformed `!!set`,
  `!!omap`, and `!!pairs` seeds plus resolved and duplicate `!!value` key
  seeds, so recent compatibility diagnostics replay through the fuzz/property
  safety gates.
- Emitter round-trip fuzz coverage now exercises `to_string`, `to_writer`, and
  streaming `Serializer` output against parsed-tree and `Value` replay.
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
  alias object identity, per-case input digests, error locations where Psych
  exposes them, and first-class alias redefinition/recursive identity probes as
  deliberate compatibility decisions.
- A Psych/libyaml coverage ledger now groups the 37 pinned probe cases into
  eight behavior families and tracks seven explicit next-probe gaps, keeping
  YAML 1.1/libyaml scope auditable without claiming blanket compatibility.
- `TaggedValue` now implements owned and borrowed Serde deserializer support
  for direct enum and `IgnoredAny` reads, matching the package-alias
  `serde_yaml::value::TaggedValue` surface.
- `LosslessEdit` now has source-backed scalar-keyed block/flow mapping entry
  helpers for value replacement, entry insertion, and entry deletion while
  preserving untouched comments, aliases, and formatting and reparsing the
  final YAML.
- `LosslessEdit` now also has source-backed block/flow sequence item helpers for
  item replacement, insertion, and deletion, with source-preserving formatting
  plus direct lossless-edit fuzz/property replay seeds.
- The YAML test-suite coverage ledger now pins the full upstream denominator at
  402 cases, maps the 131 selected cases to canonical upstream IDs, and records
  the 271 not-imported cases as explicit coverage debt.
- Fuzz release proof now has a manual `scripts/fuzz-release-sweep.sh` path that
  records target names, corpus counts, run counts, statuses, elapsed time, and
  artifact directories, while `parser_properties` gates corpus target parity,
  release floors, and named safety seeds.

Known release-candidate gaps remain tracked in `BASELINE.md`,
`COMPATIBILITY.md`, and `MIGRATION.md`: complete YAML 1.1 ecosystem parity,
full arbitrary structural lossless formatting beyond targeted block/flow
mapping entry and sequence item helpers, semantic `Node`/`Value` graph
identity, final package metadata, and external publication.
