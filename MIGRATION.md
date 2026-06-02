# serde_yaml Migration Readiness Report

This report describes the current adoption surface for moving common
config-loading code from `serde_yaml` to this crate.

Status: adoption-candidate for config-shaped Serde read paths, with structural
write support. This is not a blanket drop-in claim for every YAML document,
every emitter formatting choice, or full YAML 1.1/libyaml compatibility mode.

## Migration Shape

There are two supported rename paths for local evaluation.

### Cargo Package Alias

Use this when you want existing `serde_yaml::...` paths to keep compiling while
the dependency resolves to this crate:

```toml
[dependencies]
serde_yaml = { package = "saneyaml", path = "/Users/jk/Desktop/yaml" }
```

With this shape, the covered public surface stays spelled
`serde_yaml::from_str`, `serde_yaml::Value`, `serde_yaml::with::singleton_map`,
and so on. The package-alias smoke fixtures compile those names from a clean
downstream crate.

### Direct Crate Alias

Use this when the dependency is named `yaml`, but a source file should exercise
the same call sites as the old crate:

```toml
[dependencies]
yaml = { package = "saneyaml", path = "/Users/jk/Desktop/yaml" }
```

```rust
use yaml as serde_yaml;

let config: Config = serde_yaml::from_str(input)?;
let value: serde_yaml::Value = serde_yaml::from_slice(bytes)?;
# Ok::<(), serde_yaml::Error>(())
```

The compileable example in `examples/serde_yaml_migration.rs` uses this
direct-alias path for typed reads, `Value` patching, stream reads, structural
writes, tagged enum helpers, and diagnostic handling. The focused
`serde_yaml_direct_alias_smoke` test pins the same spelling in the normal test
suite.

Typical import rewrites:

```rust
// before
let config: Config = serde_yaml::from_str(input)?;
let value: serde_yaml::Value = serde_yaml::from_slice(bytes)?;

// after
let config: Config = yaml::from_str(input)?;
let value: yaml::Value = yaml::from_slice(bytes)?;
```

That dependency-alias path is covered by
`tests/fixtures/downstream/package-alias-smoke-strict`,
`tests/fixtures/downstream/package-alias-smoke`, and
`scripts/downstream-build-trials.sh smoke-only`. The strict smoke compiles and
runs the same upstream-compatible `serde_yaml` API calls once against
`serde_yaml 0.9.34` and once against this package under the `serde_yaml`
dependency name. The expanded smoke then covers this crate's extension surface,
including root pull event/document streaming helpers, explicit YAML 1.1
`LoadOptions`, bounded large-reader behavior with `max_input_bytes()`,
caller-built default merge deserialization plus explicit in-place merge
expansion, lossless graph identity inspection, writer paths, and diagnostic
locations. A real-world package-alias smoke copies the checked-in
GitHub Actions, Docker Compose, Kubernetes, Helm, OpenAPI, Wrangler, and
Ansible fixture registry into a clean downstream crate, parses every registered
fixture through `serde_yaml::Deserializer`, and keeps representative deep field
assertions through `serde_yaml::...` imports, including default Docker Compose
merge expansion plus Kubernetes CRD/OpenAPI schema bodies, Helm values and
dependency metadata, OpenAPI polymorphism, Wrangler durable object migrations,
and Ansible `!vault` / `!unsafe` tags. An external downstream package-alias
smoke separately copies the
checked-in Pingora, rust-i18n, cfn-guard, navi, and Stackable fixture reductions
into a clean downstream crate and exercises typed reads, reader paths,
structural emits, tagged CloudFormation `Value` access, locale trees, CLI config
defaults, and Kubernetes CRD/OpenAPI shapes through `serde_yaml::...` imports.
It also replays tagged CloudFormation values and Stackable CRDs through
`to_string`, `to_writer`, and streaming `Serializer`, then reparses the output
as equivalent structure.
These are package-resolution and runtime smoke tools, not blanket promises that
every `serde_yaml` behavior or formatting byte matches.

The low-friction path is to replace owned config reads and common
`serde_yaml::Value` usage first. Keep compatibility-sensitive code covered by
tests that exercise the actual downstream YAML files.

## Cookbook

Each recipe shows the old `serde_yaml` call site, then the direct `yaml` import
shape. If you use the Cargo package alias or `use yaml as serde_yaml;`, keep the
left-hand spelling and let the dependency or local alias do the rename.

### Typed Read

```rust
// before
let config: Config = serde_yaml::from_str(input)?;
let from_slice: Config = serde_yaml::from_slice(bytes)?;
let from_reader: Config = serde_yaml::from_reader(reader)?;

// after
let config: Config = yaml::from_str(input)?;
let from_slice: Config = yaml::from_slice(bytes)?;
let from_reader: Config = yaml::from_reader(reader)?;
# Ok::<(), yaml::Error>(())
```

### Value Indexing and Patching

```rust
// before
let mut value: serde_yaml::Value = serde_yaml::from_str(input)?;
value["services"]["api"]["image"] = serde_yaml::Value::from("nginx:latest");
let ports = value["services"]["api"]["ports"].as_sequence();

// after
let mut value: yaml::Value = yaml::from_str(input)?;
value["services"]["api"]["image"] = yaml::Value::from("nginx:latest");
let ports = value["services"]["api"]["ports"].as_sequence();
# let _ = ports;
# Ok::<(), yaml::Error>(())
```

`yaml::Sequence`, `yaml::Mapping`, `yaml::Number`, `yaml::value::*`, and
`yaml::mapping::*` also resolve under the package alias and direct-alias paths.
The `Index` traits are sealed, as they were in `serde_yaml`; use the built-in
string, `usize`, and `Value` lookup forms.

### Tagged Enums and Singleton Maps

```rust
// before
#[derive(serde::Deserialize, serde::Serialize)]
struct Job {
    #[serde(with = "serde_yaml::with::singleton_map")]
    action: Action,
}

// after, when importing the crate as yaml
#[derive(serde::Deserialize, serde::Serialize)]
struct Job {
    #[serde(with = "yaml::with::singleton_map")]
    action: Action,
}
```

Under `serde_yaml = { package = "saneyaml", ... }` or `use yaml as serde_yaml;`,
keep `#[serde(with = "serde_yaml::with::singleton_map")]`. Nested enum payloads
that need one-entry mapping form should use
`singleton_map_recursive`. The helpers reject YAML tag shorthand through those
`with` paths, matching `serde_yaml`.

### Multi-Document Streams

```rust
// before
let docs = serde_yaml::Deserializer::from_str(stream)
    .map(Config::deserialize)
    .collect::<Result<Vec<_>, _>>()?;

// after
let docs = yaml::Deserializer::from_str(stream)
    .map(Config::deserialize)
    .collect::<Result<Vec<_>, _>>()?;

// additive convenience
let docs: Vec<Config> = yaml::from_documents_str(stream)?;
# Ok::<(), yaml::Error>(())
```

`from_documents_str`, `from_documents_slice`, and `from_documents_reader` are
this crate's all-or-error convenience helpers. Use iterator-style
`Deserializer` when you need to process earlier good documents before a later
stream error.

### Structural Write

```rust
// before
let yaml_text = serde_yaml::to_string(&config)?;
serde_yaml::to_writer(&mut writer, &config)?;

// after
let yaml_text = yaml::to_string(&config)?;
yaml::to_writer(&mut writer, &config)?;
# let _ = yaml_text;
# Ok::<(), yaml::Error>(())
```

The default writer is deterministic structural YAML. Use
`yaml::EmitOptions::byte_compatible()` only for the documented byte-compatible
structural corpus; comments, anchors, directives, and source style are lossless
concerns, not default writer concerns.

### Error and Location Handling

```rust
// before
let error: serde_yaml::Error = serde_yaml::from_str::<Config>("name: [")
    .unwrap_err();
if let Some(location) = error.location() {
    eprintln!("{}:{}", location.line(), location.column());
}

// after
let error: yaml::Error = yaml::from_str::<Config>("name: [").unwrap_err();
if let Some(location) = error.location() {
    eprintln!("{}:{}", location.line(), location.column());
}
# Ok::<(), yaml::Error>(())
```

`yaml::Error::line()` and `column()` mirror the common convenience path, while
`span()`, `category()`, `path()`, `document_index()`, and `render_source(...)`
are additive diagnostics.

## API Matrix

| serde_yaml surface | yaml surface | Status |
|---|---|---|
| `serde_yaml::from_str` | `yaml::from_str` | Covered for typed config reads, `Value`, borrowed string targets, and diagnostics |
| `serde_yaml::from_slice` | `yaml::from_slice` | Covered for typed config reads, `Value`, UTF-8 errors, and borrowed string targets |
| `serde_yaml::from_reader` | `yaml::from_reader` | Covered for owned typed reads with bounded input loading; borrowed targets remain owned-only |
| `serde_yaml::Deserializer::from_str` | `yaml::Deserializer::from_str` | Covered for single-document Serde use and multi-document iteration |
| `serde_yaml::Deserializer::from_slice` | `yaml::Deserializer::from_slice` | Covered for direct Serde use and diagnostics |
| `serde_yaml::Deserializer::from_reader` | `yaml::Deserializer::from_reader` | Covered for owned direct Serde use with bounded input loading; no borrowed output from consumed readers |
| `serde_yaml::Value` | `yaml::Value` | Covered for common reads, mutation, indexing, merge expansion, tags, traits, and `Deserialize` |
| `serde_yaml::Mapping` | `yaml::Mapping` | Covered for insertion, lookup, entry API, iteration, equality, hashing, and ordering |
| `serde_yaml::Number` | `yaml::Number` | Covered for helpers, parsing, display, direct deserialization, and widened integer targets |
| `serde_yaml::value::to_value` | `yaml::value::to_value` | Covered for common config-shaped serialization |
| `serde_yaml::value::Serializer` | `yaml::value::Serializer` | Covered for value-backed serialization, bytes, tags, and 128-bit integer policy |
| `serde_yaml::to_string` | `yaml::to_string`; `yaml::to_string_with_options` | `EmitOptions::structural()` output covered as the default; `byte_compatible()` matches `serde_yaml` bytes for the supported structural writer corpus; structural style knobs are opt-in |
| `serde_yaml::to_writer` | `yaml::to_writer`; `yaml::to_writer_with_options` | `EmitOptions::structural()` output covered as the default; `byte_compatible()` writer bytes covered for the supported structural writer corpus; structural style knobs are opt-in |
| `serde_yaml::Serializer` | `yaml::Serializer` | Covered for multi-document writer usage and document marker policy; `Serializer::with_options(..., EmitOptions::structural())` matches the default writer path, and `Serializer::with_options(..., EmitOptions::byte_compatible())` matches `serde_yaml` for the supported structural stream corpus |
| `serde_yaml::with::singleton_map` | `yaml::with::singleton_map` | Covered for read and write enum-field annotations |
| `serde_yaml::with::singleton_map_recursive` | `yaml::with::singleton_map_recursive` | Covered for nested read and write enum-field annotations |
| `serde_yaml::Error` / `Result` | `yaml::Error` / `Result` | Covered for parser, Serde, writer, and direct-deserializer errors; richer diagnostics are additive |
| `serde_yaml::Location` | `yaml::Location` | Covered for `index()`, `line()`, and `column()` location handling |
| `use yaml as serde_yaml;` | local direct alias | Covered by `tests/serde_yaml_direct_alias_smoke.rs` and `examples/serde_yaml_migration.rs` |

Additional crate surfaces useful during migration:

- `yaml::LoadOptions::{core, json, failsafe, legacy_serde_yaml}` and
  `yaml::Schema::{Core, Json, Failsafe, LegacySerdeYaml}` expose named scalar
  resolution modes. `yaml::Schema::Yaml12` remains the default-compatible
  spelling for YAML 1.2-oriented Core behavior, and
  `yaml::LoadOptions::yaml_1_1()` / `yaml::Schema::Yaml11` remain retained
  spellings for the broad legacy YAML 1.1/libyaml-era mode. Legacy construction
  resolves boolean/null aliases, timestamp-shaped plain scalars, legacy radix
  and sexagesimal numeric spellings for callers that know their corpus depends
  on those rules. `yaml::LoadOptions::yaml_version_directive()` and
  `yaml::Schema::YamlVersionDirective` apply that legacy construction per
  document only when the document declares `%YAML 1.1`. Default entrypoints
  remain YAML 1.2-oriented.
- `yaml::LoadOptions` enforces a 64 MiB input byte ceiling by default across
  string, slice, reader, pull event/document streams, and direct deserializer
  paths. `yaml::parse_lossless_bytes` applies the same default ceiling before
  UTF-8 validation. Use `max_input_bytes()` to tune the ceiling for a loader
  call site, `max_alias_expansion_nodes()` to tune alias expansion work for
  untrusted config loads, or `without_input_limit()` only when a caller has
  already bounded the source. Raw event streaming validates aliases without
  expanding them; document streaming uses the same alias-expansion budget as
  loaded-tree and Serde paths.
- `yaml::from_node` preserves parser spans while deserializing from a loaded tree.
- `yaml::from_documents_str`, `from_documents_slice`, and
  `from_documents_reader` return typed vectors for YAML streams.
- `yaml::stream::{EventStream, DocumentStream}` plus root
  `stream_events*` / `stream_documents*` helpers expose pull-based parser-event
  and one-document-at-a-time loading surfaces that `serde_yaml` does not
  provide directly. `parse_events` and `parse_documents` remain all-or-error
  convenience collection APIs over the same parser behavior.
- `yaml::parse_lossless` and `yaml::LosslessStream` provide a separate
  source-backed graph surface for callers that need byte-stable replay,
  comments/trivia, scalar spelling, directives, alias-to-anchor identity checked
  against `yaml-rust2` and `saphyr` parser anchor events for manifest-owned
  selected YAML-suite anchor/alias cases and real-world graph fixtures,
  merge-effective mapping inspection that retains raw `<<` source and
  alias/anchor provenance, and validated node/source-span edits, insertions,
  and deletions that preserve untouched bytes.

## Executable Proof

`tests/serde_yaml_swap_harness.rs` is the migration-facing proof harness. It
currently covers:

- typed config reads through `from_str`, `from_slice`, `from_reader`, and direct
  `Deserializer` use
- direct `IgnoredAny` deserialization that still validates malformed input and
  single-document boundaries before skipping
- stream document iteration
- `Value`, `Mapping`, `Number`, `Tag`, and `TaggedValue` patch-style and
  direct deserializer usage
- `to_value`, `to_string`, and `to_writer` structural writer paths
- `with::singleton_map` enum field annotations, including upstream-style
  rejection of YAML tag shorthand through those helper paths
- default untagged and explicit merge-tag expansion for parsed and caller-built
  `Value` deserialization plus idempotent `Value::apply_merge` as an in-place
  helper
- value-backed bytes and writer byte rejection policy
- empty input and empty stream behavior
- the default merge-key migration decision: parsed `yaml::Value`, `from_value`,
  and direct owned/borrowed `Value` Serde reads expand `<<`, while
  `serde_yaml::Value` keeps the literal key until `apply_merge()`
- real-world GitHub Actions, Docker Compose, Kubernetes, Helm, OpenAPI,
  Wrangler, and Ansible fixture fields compared against `serde_yaml`, including
  CRD schemas, Helm values/dependencies, OpenAPI polymorphism, Wrangler durable
  object migrations, and Ansible vault/unsafe tags

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
- `denisidoro/navi` / `navi` 2.25.0-beta1 at commit
  `1ac218cb1e0e80649ef23c8a916e67efc3086833`, Apache-2.0, covering typed CLI
  configuration loaded through `serde_yaml::from_str` and
  `serde_yaml::from_reader`, nested defaults, shell command strings, and
  commented example config files.
- `stackabletech/operator-rs` / `stackable-operator` 0.111.1 at commit
  `fd86c0ebf9b885be2684d7d867d513ab9df8c2e1`, Apache-2.0, covering
  Kubernetes CustomResourceDefinition YAML with nested OpenAPI schemas,
  `oneOf` variants, defaulted values, and `x-kubernetes-*` extension fields.

`scripts/downstream-build-trials.sh pingora`,
`scripts/downstream-build-trials.sh rust-i18n`,
`scripts/downstream-build-trials.sh cfn-guard`,
`scripts/downstream-build-trials.sh navi`,
`scripts/downstream-build-trials.sh stackable-operator`,
`scripts/downstream-build-trials.sh figment`, and
`scripts/downstream-build-trials.sh uaparser` add real downstream build trials.
Each packages this crate, consumes the unpacked package from a clean smoke
project under the `serde_yaml` dependency name, runs strict upstream-compatible
expanded alias-surface assertions, parses representative checked-in real-world
config fixtures, and replays the checked-in external downstream fixture
reductions against that package, then checks a pinned downstream checkout with
its `serde_yaml` dependency rewritten to that packaged copy. The Pingora trial
checks `pingora-core` plus the `pingora-proxy` `modify_response` example that
uses `serde_yaml` as a dev dependency; the rust-i18n trial covers support,
macro, and extract crates; the cfn-guard trial checks the package that loads
CloudFormation templates and rule-test specs; the navi trial checks the library
and CLI binary that load typed YAML config through string and reader paths; the
Stackable trial checks `stackable-shared` production serializer use plus
`k8s-version` serde tests. The figment trial copies the crates.io 0.10.19
package source, rewrites its optional table-style `serde_yaml` dependency to
the packaged alias, checks the `yaml` provider feature, and runs the YAML enum
provider test. The uaparser trial copies the crates.io 0.6.4 package source,
rewrites its table-style `serde_yaml` dependency to the packaged alias, runs
library tests over the bundled `regexes.yaml` database through slice and reader
paths, and checks the examples that build parsers from that YAML file.

Focused proof command:

```sh
cargo test --test serde_yaml_swap_harness --test downstream_migration_harness
cargo test --test external_downstream_migration
cargo test --test libyaml_probe_manifest
scripts/downstream-build-trials.sh smoke-only
scripts/downstream-build-trials.sh pingora
scripts/downstream-build-trials.sh rust-i18n
scripts/downstream-build-trials.sh cfn-guard
scripts/downstream-build-trials.sh navi
scripts/downstream-build-trials.sh stackable-operator
scripts/downstream-build-trials.sh figment
scripts/downstream-build-trials.sh uaparser
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
33-file / 39-document real-world registry without timing file I/O:

```sh
cargo run --release --example real_world_benchmark
```

The latest captured table is recorded in `BENCHMARKS.md`.

## Real-World Fixture Coverage

Current real-world gates cover 33 files / 39 YAML documents across:

- GitHub Actions
- Docker Compose
- Kubernetes
- Helm
- OpenAPI
- Wrangler
- Ansible
- CloudFormation/SAM
- Symfony services
- GitLab CI
- CircleCI
- Azure Pipelines

These fixtures prove config-shaped parsing, Serde reads, event/tree parity, and
reference acceptance for the selected suite. Docker Compose merge-anchor
fixtures are tree-parity checked after normalizing reference-loader trees with
this crate's default merge expansion policy, while raw event and lossless graph
tests keep the original `<<` syntax visible. They are not a substitute for
testing each adopter's own YAML corpus.

## Required Call-Site Changes

- With `serde_yaml = { package = "saneyaml", ... }`, keep existing
  `serde_yaml::...` paths for the covered public surface and let Cargo resolve
  that name to this crate.
- With `use yaml as serde_yaml;`, keep the old spelling inside that source file
  while depending on `yaml`.
- With direct `yaml::...` imports, mechanically replace the prefix:
  `serde_yaml::Value` becomes `yaml::Value`, `serde_yaml::Mapping` becomes
  `yaml::Mapping`, `serde_yaml::Number` becomes `yaml::Number`,
  `serde_yaml::with::singleton_map` becomes `yaml::with::singleton_map`, and
  `serde_yaml::Error` becomes `yaml::Error`.
- Parser and Serde errors expose line/column locations. Spanless `Value` and
  reader I/O errors cannot recover source spans.
- Treat writer output as `EmitOptions::structural()` YAML by default. Select
  `EmitOptions::byte_compatible()` only for the proven `serde_yaml` byte
  corpus: common scalars, maps, sequences, Serde enum tags, document markers,
  typed real-world config writer shapes, and bytes rejection. Structural style
  knobs can sort keys, choose scalar quote style, prefer literal or folded
  block scalars where representable, and choose block or flow collections.
  Comments, original source style, anchors/aliases, directives, and arbitrary
  lossless formatting are not byte-compatible migration surfaces; use
  `LosslessStream` for source-preserving replay.

## Known Migration Limits

- Schema resolution is explicit. `LoadOptions::core()` follows the default
  YAML 1.2-oriented scalar table, `LoadOptions::json()` resolves only JSON
  lowercase booleans/null and JSON numbers while leaving other scalar text as
  strings, `LoadOptions::failsafe()` leaves scalar text as strings, and
  `LoadOptions::legacy_serde_yaml()` follows the existing broad legacy
  YAML 1.1/libyaml-era table. Missing mapping values remain parser empty nodes
  before schema resolution and therefore stay null. The full table is in
  `COMPATIBILITY.md`.
- YAML 1.1 scalar construction is explicit. `LoadOptions` can resolve legacy
  boolean/null aliases plus timestamp-shaped plain scalars, signed and
  underscored leading-zero octal, hex, binary numeric, two/three-part
  sexagesimal int/float forms, and numeric forms that fit `yaml::Number`, while
  oversized numeric spellings stay strings. Timestamps keep `!!timestamp`
  tag/source metadata in `Value`/`Node` and expose `yaml::Timestamp` through
  `as_timestamp()` and typed Serde reads. `!!binary` payloads, including
  whitespace-separated payloads, are retained as tagged strings in `Value`/`Node`
  while decoding for typed byte targets such as `Vec<u8>`,
  `deserialize_bytes`, and `deserialize_byte_buf`; malformed payloads reject
  typed byte targets rather than failing retained tree loading. Explicit
  `!!int` and `!!float` retained `Value`
  entries keep their tag and source spelling, but valid YAML 1.1 numeric forms
  are visible through `Value` numeric helpers such as `as_i64()`, `as_u64()`,
  `as_f64()`, and `is_number()`. The supported explicit core tags may also be
  written with canonical YAML URI tags such as `!<tag:yaml.org,2002:int>` or
  declared `%TAG` handles that resolve to `tag:yaml.org,2002:*`.
  Directive-driven loading is available through
  `LoadOptions::yaml_version_directive()`, where `%YAML 1.1` selects the legacy
  construction mode and absent, `%YAML 1.2`, or newer numeric directives keep
  YAML 1.2-oriented construction. Default loading still treats decimal-looking
  leading-zero scalars such as `0123` as decimal integers; YAML 1.1 opt-in
  treats the same spelling as octal. YAML 1.1 opt-in also follows
  Psych/libyaml merge-edge recovery for repeated real merge keys and
  non-mergeable merge payloads; default YAML 1.2-oriented loading keeps those
  edges strict. `tests/yaml11_conformance.rs` includes
  directive-driven migration fixtures covering legacy boolean words, null
  spellings, float spellings, octal, hex, binary numeric, sexagesimal,
  oversized numeric spellings, timestamp time-zone and leap-second forms,
  flow-style scalar collections and mapping keys, explicit binary whitespace,
  invalid binary typed-target diagnostics, collection and
  structural tags, merge-key expansion, boolean and numeric key collisions,
  signed-zero key collisions, and alias-expanded duplicate-key diagnostics.
- YAML 1.1 collection and structural tags are retained as tagged payloads in
  `Node` and `Value`, not converted to new public value variants. Typed Serde
  reads understand `!!set` as set-like sequence targets from mapping keys,
  `!!omap` as ordered pair sequences or map targets, `!!pairs` as pair
  sequences that preserve duplicate keys, `!!seq` as sequence targets, `!!map`
  as map/struct targets, and `!!value` as the scalar value, including custom
  `%TAG` handles that resolve to those YAML core tags. Non-null `!!set` entry
  values and non-singleton `!!omap`/`!!pairs` entries are rejected for those
  typed reads instead of being silently dropped or flattened.
- Untagged and explicit `!!merge` / canonical merge-tag keys are expanded by
  default in loaded trees, `from_value`, and direct owned/borrowed `Value`
  Serde reads. `Value::apply_merge()` remains available as an explicit
  in-place helper and is idempotent for values parsed by this crate. Explicit
  `!!str <<` and custom-tagged `<<` keys stay literal.
- `yaml::Deserializer::from_str("")`, `from_slice(b"")`, and
  `from_reader(empty)` yield one null document, matching
  `serde_yaml::Deserializer::from_str("")`. Direct `from_str::<Value>("")` and
  direct `Value::deserialize(...)` also treat empty input as null in both crates.
- Aliases are expanded into semantic `Node`/`Value` loaded trees; graph identity
  is preserved only through the separate `LosslessStream` API.
- Comments and original formatting are discarded by semantic `Node`/`Value`
  loaders, but retained by `LosslessStream` for source-backed replay, graph
  inspection, and validated source-span edits through `LosslessEdit`.
- `yaml::Index` and `yaml::mapping::Index` are sealed, like `serde_yaml`'s
  indexing traits. Downstream code should use the normal string, `usize`, and
  `Value` lookup APIs rather than implementing indexing as an extension point.
  `usize` indexes `Value` sequences and numeric mapping keys; direct
  `Mapping` indexing accepts string-like keys or `Value` keys, not sequence
  positions.
- Full upstream YAML test-suite coverage is now classified; the pinned coverage
  ledger records 402 upstream cases, 402 selected cases, and 0 not-imported
  cases, while selected-suite scope and deferred parity cases remain documented
  in `BASELINE.md` and `COMPATIBILITY.md`. `cargo test --test
  conformance_dashboard -- --nocapture` prints the current 402-case dashboard
  and keeps documented divergence overlays separate from accepted/rejected
  outcome counts.

## Migration Impact Ledger

| Area | Migration impact |
|---|---|
| Default merge expansion | Parsed `Node`/`Value`, `from_value`, direct owned/borrowed `Value` deserializers, and other Serde reads expand untagged and explicit merge-tag `<<` keys by default. Code that inspected merge syntax should switch to `parse_events`, `LosslessStream`, `LosslessStream::effective_mapping_entries`, or inspect caller-built `Value` before deserializing; explicit `!!str <<` and custom-tagged `<<` keys remain literal. |
| YAML 1.1 compatibility | Legacy scalar, collection, and merge-edge recovery behavior is available through explicit schema/tag paths. Default entrypoints stay YAML 1.2-oriented, so corpora that require YAML 1.1 typing or Psych-style repeated/invalid merge recovery need opt-in tests. |
| Alias graph identity | Semantic `Node`/`Value` trees intentionally clone acyclic aliases and reject recursive alias expansion. Graph-sensitive callers should use `LosslessStream`; its anchor definitions and alias targets are checked against reference parser anchor events for redefinition, recursive, document-reset, merge, YAML 1.1 merge/comment graph fixtures, post-edit source output, manifest-owned selected YAML-suite anchor/alias cases, and manifest-owned real-world Docker Compose anchor cases including an adapted official Compose Specification fragment. `LosslessStream::effective_mapping_entries` exposes merge-derived entries with alias/anchor provenance for callers that need effective config inspection without losing graph identity. |
| Lossless formatting | `LosslessStream` preserves source, comments, trivia, directives, anchors, aliases, tags, and scalar spelling for replay/inspection, including a merge-effective mapping view that leaves the original source untouched. `LosslessEdit` can replace retained node or raw source spans, update scalar-keyed block/flow mapping values, insert or delete block/flow mapping entries, update block/flow sequence items, insert or delete block/flow sequence items, insert source, delete source spans, and validate the final YAML while preserving untouched bytes. Manifest-owned real-world replay now gates GitHub Actions comments, flow-style lists, and expression strings, Ansible tagged scalars, plus Kubernetes streams and block scalar fixtures. |
| Parser acceptance differences | Some YAML 1.2 inputs rejected by libyaml are accepted, and some malformed libyaml-tolerated inputs are rejected. Divergence records now carry per-case migration impact. |
| Package readiness | The package is prepared as `saneyaml` 0.1.0 under MIT and remains unpublished until explicit crates.io publish approval is given. |

## Next Adoption Blockers

- Continue expanding real external crate build trials beyond the current
  Pingora, rust-i18n, cfn-guard, navi, Stackable operator-rs, figment, and
  uaparser package smoke before claiming broad ecosystem replacement readiness.
- Keep migration-impact wording current as new divergence records are added.
- Keep growing default merge, `apply_merge`, emitter, and lossless graph
  coverage with sustained fuzz runs and minimized discoveries beyond the
  curated seed corpus. `scripts/fuzz-release-sweep.sh` is the release-audit path
  for recording checkout HEAD/status, per-target corpus counts, run counts,
  elapsed time, statuses, and artifact directories before a release candidate;
  unfiltered sweeps must cover every configured fuzz target.
- Keep broader YAML 1.1/libyaml compatibility decisions explicit beyond the
  fixture-backed Psych/libyaml merge/tag/graph cross-checks and eight-family
  Psych/libyaml coverage ledger, now including directive stream-boundary
  behavior and no tracked next-probe gaps; full arbitrary structural lossless formatting/emission beyond
  targeted block/flow mapping entry and sequence item helpers; and the
  long-term graph API contract before claiming full YAML compatibility.
- Drive the next phase from the conformance dashboard parity ledgers: event
  parity is closed, and explicit core scalar tag fixtures now count in
  loaded-tree value-shape parity under semantic explicit-core projection while
  retained `Node`/`Value` trees still preserve tag metadata. Remaining
  loaded-tree and shared-reference deferrals need reference-policy changes
  before promotion.
  The broad shared-reference
  catch-all has been split into case-family records for flow collection syntax,
  stream-marker and empty-key document shapes, unusual anchor characters, and
  A2M4 block indentation, so the remaining work is policy rather than missing
  record ownership.
- Obtain explicit approval before running the real crates.io publish.
