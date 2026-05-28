# serde_yaml Migration Readiness Report

This report describes the current adoption surface for moving common
config-loading code from `serde_yaml` to this crate.

Status: adoption-candidate for config-shaped Serde read paths, with structural
write support. This is not a blanket drop-in claim for every YAML document,
every emitter formatting choice, or full YAML 1.1/libyaml compatibility mode.

## Migration Shape

For local evaluation:

```toml
[dependencies]
yaml = { path = "/Users/jk/Desktop/yaml" }
```

The compileable example in `examples/serde_yaml_migration.rs` shows the same
path for typed reads, `Value` patching, stream reads, structural writes, and
diagnostic handling.

Typical import rewrites:

```rust
// before
let config: Config = serde_yaml::from_str(input)?;
let value: serde_yaml::Value = serde_yaml::from_slice(bytes)?;

// after
let config: Config = yaml::from_str(input)?;
let value: yaml::Value = yaml::from_slice(bytes)?;
```

The low-friction path is to replace owned config reads and common
`serde_yaml::Value` usage first. Keep compatibility-sensitive code covered by
tests that exercise the actual downstream YAML files.

## API Matrix

| serde_yaml surface | yaml surface | Status |
|---|---|---|
| `serde_yaml::from_str` | `yaml::from_str` | Covered for typed config reads, `Value`, borrowed string targets, and diagnostics |
| `serde_yaml::from_slice` | `yaml::from_slice` | Covered for typed config reads, `Value`, UTF-8 errors, and borrowed string targets |
| `serde_yaml::from_reader` | `yaml::from_reader` | Covered for owned typed reads; borrowed targets remain owned-only |
| `serde_yaml::Deserializer::from_str` | `yaml::Deserializer::from_str` | Covered for single-document Serde use and multi-document iteration |
| `serde_yaml::Deserializer::from_slice` | `yaml::Deserializer::from_slice` | Covered for direct Serde use and diagnostics |
| `serde_yaml::Deserializer::from_reader` | `yaml::Deserializer::from_reader` | Covered for owned direct Serde use; no borrowed output from consumed readers |
| `serde_yaml::Value` | `yaml::Value` | Covered for common reads, mutation, indexing, merge expansion, tags, traits, and `Deserialize` |
| `serde_yaml::Mapping` | `yaml::Mapping` | Covered for insertion, lookup, entry API, iteration, equality, hashing, and ordering |
| `serde_yaml::Number` | `yaml::Number` | Covered for helpers, parsing, display, direct deserialization, and widened integer targets |
| `serde_yaml::value::to_value` | `yaml::value::to_value` | Covered for common config-shaped serialization |
| `serde_yaml::value::Serializer` | `yaml::value::Serializer` | Covered for value-backed serialization, bytes, tags, and 128-bit integer policy |
| `serde_yaml::to_string` | `yaml::to_string` | Structural output covered; byte-for-byte formatting parity is out of scope |
| `serde_yaml::to_writer` | `yaml::to_writer` | Structural output covered; byte-for-byte formatting parity is out of scope |
| `serde_yaml::Serializer` | `yaml::Serializer` | Covered for multi-document writer usage and document marker policy |
| `serde_yaml::with::singleton_map` | `yaml::with::singleton_map` | Covered for read and write enum-field annotations |
| `serde_yaml::with::singleton_map_recursive` | `yaml::with::singleton_map_recursive` | Covered for nested read and write enum-field annotations |

Additional crate surfaces useful during migration:

- `yaml::LoadOptions::yaml_1_1()` and `yaml::Schema::Yaml11` opt into legacy
  YAML 1.1 boolean/null and numeric scalar construction, including legacy
  radix and sexagesimal numeric spellings, for callers that know their corpus
  depends on those rules. `yaml::LoadOptions::yaml_version_directive()` and
  `yaml::Schema::YamlVersionDirective` apply that legacy construction per
  document only when the document declares `%YAML 1.1`. Default entrypoints
  remain YAML 1.2-oriented.
- `yaml::from_node` preserves parser spans while deserializing from a loaded tree.
- `yaml::from_documents_str`, `from_documents_slice`, and
  `from_documents_reader` return typed vectors for YAML streams.
- `yaml::parse_events` and `yaml::parse_documents` expose parser/event proof
  surfaces that `serde_yaml` does not provide directly.
- `yaml::parse_lossless` and `yaml::LosslessStream` provide a separate
  source-backed graph surface for callers that need byte-stable replay,
  comments/trivia, scalar spelling, directives, alias-to-anchor identity, and
  validated node source edits that preserve untouched bytes.

## Executable Proof

`tests/serde_yaml_swap_harness.rs` is the migration-facing proof harness. It
currently covers:

- typed config reads through `from_str`, `from_slice`, `from_reader`, and direct
  `Deserializer` use
- stream document iteration
- `Value`, `Mapping`, and `Number` patch-style usage
- `to_value`, `to_string`, and `to_writer` structural writer paths
- `with::singleton_map` enum field annotations
- default merge-key expansion plus idempotent `Value::apply_merge` for
  caller-built values
- value-backed bytes and writer byte rejection policy
- empty input and empty stream behavior
- the default merge-key migration decision: parsed `yaml::Value` expands `<<`
  while `serde_yaml::Value` keeps the literal key until `apply_merge()`
- real-world GitHub Actions, Docker Compose, Kubernetes, Helm, OpenAPI,
  Wrangler, and Ansible fixture fields compared against `serde_yaml`

`tests/divergence_manifest.rs` also gates the divergence registry. Every record
under `tests/fixtures/divergences/records/` must include `migration_impact`
text, so intentional behavior splits stay tied to caller-facing adoption risk.

`tests/downstream_migration_harness.rs` adds downstream-shaped typed call sites
for GitHub Actions, Docker Compose, Kubernetes streams, Helm, OpenAPI,
Wrangler, and Ansible, and compares each result against `serde_yaml`.

`tests/external_downstream_migration.rs` adds pinned external replay fixtures
from real `serde_yaml` users:

- `cloudflare/pingora` at commit
  `c0845a8693b0792a6ccd0626e8475990f7269af2`, Apache-2.0, covering typed
  server/proxy configuration reads and structural rewrites.
- `longbridge/rust-i18n` at commit
  `97cf091c24e4bc09a0acb397a8d9d7da8b6abc56`, MIT, covering locale maps,
  nested translation trees, Unicode text, and interpolation placeholders.
- `aws-cloudformation/cloudformation-guard` / `cfn-guard` 3.2.0 at commit
  `ae35f4e6a5618ffb1f3653c084c450f82fc2fc51`, Apache-2.0, covering
  CloudFormation templates and cfn-guard rule-test specs loaded through
  `serde_yaml::Value`, including short-form intrinsic tags such as `!Ref`,
  `!GetAtt`, and `!Sub`.

`scripts/downstream-build-trials.sh pingora`,
`scripts/downstream-build-trials.sh rust-i18n`, and
`scripts/downstream-build-trials.sh cfn-guard` add real downstream build trials.
Each packages this crate, consumes the unpacked package from a clean smoke
project under the `serde_yaml` dependency name, then checks a pinned downstream
checkout with its `serde_yaml` dependency rewritten to that packaged copy. The
Pingora trial checks `pingora-core` plus the `pingora-proxy` `modify_response`
example that uses `serde_yaml` as a dev dependency; the rust-i18n trial covers
support, macro, and extract crates; the cfn-guard trial checks the package that
loads CloudFormation templates and rule-test specs.

Focused proof command:

```sh
cargo test --test serde_yaml_swap_harness --test downstream_migration_harness
cargo test --test external_downstream_migration
cargo test --test libyaml_probe_manifest
scripts/downstream-build-trials.sh pingora
scripts/downstream-build-trials.sh rust-i18n
scripts/downstream-build-trials.sh cfn-guard
```

Broader migration proof:

```sh
cargo test --test serde_yaml_swap_harness --test serde_value_api --test compatibility_harness --test real_world_configs
cargo test --test yaml_test_suite --test event_parity --test tree_parity --test parity_manifest
cargo test --test divergence_manifest --test divergences --test baseline_audit
cargo clippy --all-targets -- -D warnings
```

## Performance Evidence

`examples/real_world_benchmark.rs` benchmarks parse/load cost over the same
26-file / 32-document real-world registry without timing file I/O:

```sh
cargo run --release --example real_world_benchmark
```

The latest captured table is recorded in `BENCHMARKS.md`.

## Real-World Fixture Coverage

Current real-world gates cover 26 files / 32 YAML documents across:

- GitHub Actions
- Docker Compose
- Kubernetes
- Helm
- OpenAPI
- Wrangler
- Ansible

These fixtures prove config-shaped parsing, Serde reads, event/tree parity, and
reference acceptance for the selected suite. They are not a substitute for
testing each adopter's own YAML corpus.

## Required Call-Site Changes

- Replace `serde_yaml::Value`, `serde_yaml::Mapping`, and
  `serde_yaml::Number` imports with `yaml::Value`, `yaml::Mapping`, and
  `yaml::Number`.
- Replace `serde_yaml::with::singleton_map` and
  `serde_yaml::with::singleton_map_recursive` attribute paths with the matching
  `yaml::with` paths.
- Replace `serde_yaml::Error` handling with `yaml::Error`. Parser and Serde
  errors expose line/column locations, but spanless `Value` and reader I/O
  errors cannot recover source spans.
- Treat writer output as structural YAML. Do not compare emitted bytes against
  `serde_yaml` formatting.

## Known Migration Limits

- YAML 1.1 scalar construction is explicit and incomplete. `LoadOptions` can
  resolve legacy booleans/nulls plus timestamp-shaped plain scalars,
  leading-zero octal, hex, binary numeric, two/three-part sexagesimal int/float
  forms, and underscored numeric forms that fit `yaml::Number`. Timestamps keep
  `!!timestamp` tag/source metadata in `Value`/`Node` and expose
  `yaml::Timestamp` through `as_timestamp()` and typed Serde reads. `!!binary`
  payloads are retained as tagged strings in `Value`/`Node` while decoding for
  typed byte targets such as `Vec<u8>`, `deserialize_bytes`, and
  `deserialize_byte_buf`. Explicit `!!int` and `!!float` retained `Value`
  entries keep their tag and source spelling, but valid YAML 1.1 numeric forms
  are visible through `Value` numeric helpers such as `as_i64()`, `as_u64()`,
  `as_f64()`, and `is_number()`. The supported explicit core tags may also be
  written with canonical YAML URI tags such as `!<tag:yaml.org,2002:int>` or
  declared `%TAG` handles that resolve to `tag:yaml.org,2002:*`.
  Directive-driven loading is available through
  `LoadOptions::yaml_version_directive()`, where `%YAML 1.1` selects the legacy
  construction mode and absent, `%YAML 1.2`, or newer numeric directives keep
  YAML 1.2-oriented construction.
- YAML 1.1 collection tags are retained as tagged collections in `Node` and
  `Value`, not converted to new public value variants. Typed Serde reads
  understand `!!set` as set-like sequence targets from mapping keys, `!!omap`
  as ordered pair sequences or map targets, and `!!pairs` as pair sequences
  that preserve duplicate keys. Non-null `!!set` entry values and non-singleton
  `!!omap`/`!!pairs` entries are rejected for those typed reads instead of being
  silently dropped or flattened.
- Untagged merge keys are expanded by default in loaded trees and Serde reads.
  `Value::apply_merge()` remains available for caller-built values and is
  idempotent for values parsed by this crate.
- `yaml::Deserializer::from_str("")`, `from_slice(b"")`, and
  `from_reader(empty)` yield one null document, matching
  `serde_yaml::Deserializer::from_str("")`. Direct `from_str::<Value>("")` and
  direct `Value::deserialize(...)` also treat empty input as null in both crates.
- Aliases are expanded into semantic `Node`/`Value` loaded trees; graph identity
  is preserved only through the separate `LosslessStream` API.
- Comments and original formatting are discarded by semantic `Node`/`Value`
  loaders, but retained by `LosslessStream` for source-backed replay, graph
  inspection, and validated source-fragment edits through `LosslessEdit`.
- `yaml::Index` and `yaml::mapping::Index` are sealed, like `serde_yaml`'s
  indexing traits. Downstream code should use the normal string, `usize`, and
  `Value` lookup APIs rather than implementing indexing as an extension point.
  `usize` indexes `Value` sequences and numeric mapping keys; direct
  `Mapping` indexing accepts string-like keys or `Value` keys, not sequence
  positions.
- Full upstream YAML test-suite coverage is not claimed; selected-suite scope
  and deferred parity cases remain documented in `BASELINE.md` and
  `COMPATIBILITY.md`.

## Migration Impact Ledger

| Area | Migration impact |
|---|---|
| Default merge expansion | Parsed `Node`/`Value`/Serde reads expand untagged `<<` by default. Code that inspected literal merge keys should switch to `parse_events` or `LosslessStream`. |
| YAML 1.1 compatibility | Legacy scalar and collection behavior is available through explicit schema/tag paths. Default entrypoints stay YAML 1.2-oriented, so corpora that require YAML 1.1 typing need opt-in tests. |
| Alias graph identity | Semantic `Node`/`Value` trees still clone acyclic aliases. Graph-sensitive callers should use `LosslessStream` until a semantic graph-preserving API is finalized. |
| Lossless formatting | `LosslessStream` preserves source, comments, trivia, directives, anchors, aliases, tags, and scalar spelling for replay/inspection. `LosslessEdit` can replace retained node source spans and validates the final YAML while preserving untouched bytes. |
| Parser acceptance differences | Some YAML 1.2 inputs rejected by libyaml are accepted, and some malformed libyaml-tolerated inputs are rejected. Divergence records now carry per-case migration impact. |
| Package readiness | The crate remains local-preview only until public name, license, version, and crates.io approval are selected by the user. |

## Next Adoption Blockers

- Expand real external crate build trials beyond the current Pingora,
  rust-i18n, and cfn-guard package smoke before claiming broad ecosystem
  replacement readiness.
- Keep migration-impact wording current as new divergence records are added.
- Keep growing default merge and `apply_merge` coverage with sustained fuzz
  runs and minimized discoveries beyond the curated seed corpus.
- Finish broader YAML 1.1/libyaml compatibility decisions, full structural
  lossless formatting/emission beyond source-fragment replacement, and the
  long-term graph API contract before claiming full YAML compatibility.
- Choose the public package name and final license before publishing.
