# Compatibility

This is the exhaustive compatibility and divergence reference. For everyday use
you don't need it — see [Schema modes](schema-modes.md),
[Untrusted input](untrusted-input.md), and [Migrating from
serde_yaml](MIGRATION.md).

## What this crate targets

- **Primary API:** `serde_yaml` read-side ergonomics for config-shaped YAML —
  `from_str` / `from_slice` / `from_reader`, plus `Value` and structural writes.
- **Parser reference:** YAML 1.2 tree/event acceptance comparable to `yaml-rust2`
  and `saphyr` for supported syntax.
- **Documented divergence:** libyaml / YAML 1.1-era behavior is version-pinned
  against a Ruby Psych 3.1.0 / libyaml 0.2.1 probe. Default loading is YAML
  1.2-oriented; YAML 1.1 typing is opt-in.

Every divergence record under `tests/fixtures/divergences/records/` carries a
`migration_impact` field, and `tests/divergence_manifest.rs` fails any record
that omits caller-facing impact. That registry is the source of truth for
intentional behavior splits.

## serde_yaml 0.9 rename support matrix

"Supported" means the name resolves under both `serde_yaml = { package =
"saneyaml", ... }` and `use saneyaml as serde_yaml;`. "Intentionally divergent"
means it resolves but behaves differently by policy. "Not preservable" means it
isn't a stable surface this crate emulates.

| `serde_yaml` 0.9 surface | Status |
|---|---|
| `from_str`, `from_slice`, `from_reader` | Supported |
| `from_value`, `to_value` | Supported |
| `to_string`, `to_writer` | Supported (byte-identical output is an opt-in tier) |
| `Deserializer::{from_str,from_slice,from_reader}` | Supported, incl. multi-document iteration |
| `Serializer::{new,flush,into_inner}` | Supported |
| `Value`, `Sequence`, `Mapping`, `Number` | Supported |
| `value::*`, `mapping::*` | Supported |
| `with::singleton_map`, `with::singleton_map_recursive` | Supported |
| Default tag-style enum input/output | Supported |
| `Error`, `Result`, `Location` | Supported (richer diagnostics are additive) |
| `Value` merge-key retention | **Intentionally divergent** — loaded `Value` expands `<<` by default; raw events / lossless preserve it |
| Default YAML 1.1 scalar construction | **Intentionally divergent** — default is YAML 1.2; use `LoadOptions` schema modes for legacy typing |
| Exact `Number` private representation | Not preservable (public helpers kept; integers widened) |
| Downstream `impl Index` | Not preservable (sealed here, as upstream) |
| Byte-identical libyaml emitter output | Not preservable (writer is structural; bytes covered for the documented corpus) |
| Comments / anchors / graph identity in `Value` | Not preservable (use `LosslessStream`) |

## Reproducible loader matrix

Generated from `tests/fixtures/compatibility-matrix/manifest.toml` and checked by
`tests/compatibility_matrix.rs`. Cross-ecosystem entries are pinned offline
vectors; the Rust test validates their metadata and does not execute Go,
Python, or C++ runtimes.

<!-- compatibility-matrix:start -->
| Behavior family | Proof source | `yaml` policy | `yaml` | `serde_yaml` | `yaml-rust2` | `saphyr` | Cross-ecosystem vector | Divergence / migration impact |
|---|---|---|---|---|---|---|---|---|
| Typed Serde config entrypoints | tests/compatibility_matrix.rs typed AppConfig probe | YAML 1.2 default typed reads preserve common config scalars. | accept | accept | n/a | n/a | n/a | Serde-only Rust API row; parser-only loaders are intentionally marked n/a instead of given adapter shims. |
| Registered real-world fixtures | tests/fixtures/real-world/SOURCE.toml, 33 files / 39 documents | Every registered fixture must parse with the three Rust reference loaders. | accept | accept | accept | accept | n/a | Config migration smoke coverage includes CloudFormation/SAM, Symfony, GitLab CI, CircleCI, Azure Pipelines, GitHub Actions, Docker Compose, Kubernetes, Helm, OpenAPI, Wrangler, and Ansible without compatibility fallbacks. |
| CI expression and script scalars | GitHub Actions, CircleCI, and Azure Pipelines synthetic scalar shapes | Treat CI expressions as plain or quoted strings under the default schema. | accept | accept | accept | accept | go-yaml gopkg.in/yaml.v3 v3.0.1: accept<br>PyYAML 6.0.2: accept<br>yaml-cpp 0.8.0: accept | CI users can migrate expression-heavy config without enabling YAML 1.1 compatibility or expression-specific parsing. |
| Anchors, aliases, and merge keys | GitLab CI-style defaults and merge expansion fixture | Semantic loaders expand acyclic merge keys; raw/lossless surfaces preserve anchor and merge syntax. | accept | accept | accept | accept | go-yaml gopkg.in/yaml.v3 v3.0.1: accept<br>PyYAML 6.0.2: accept<br>yaml-cpp 0.8.0: accept | tests/fixtures/divergences/records/merge-keys.toml; Graph-sensitive callers should use lossless graph APIs; semantic config callers get effective merged mappings. |
| Application custom tags | CloudFormation/SAM and Symfony short-form tags | Retain application tags in this crate's Value/event/lossless surfaces while allowing common loader acceptance. | accept | accept | accept | accept | n/a | tests/fixtures/divergences/records/custom-tags.toml; Tagged config users should assert tag-retention behavior directly because some reference trees accept syntax while dropping or reshaping tag metadata. |
| Multi-document streams | Kubernetes-style explicit document stream | Explicit stream boundaries are accepted and document counts stay stable. | accept | accept | accept | accept | go-yaml gopkg.in/yaml.v3 v3.0.1: accept<br>PyYAML 6.0.2: accept<br>yaml-cpp 0.8.0: accept | Stream-processing callers should keep asserting document counts when migrating Kubernetes-style manifests. |
<!-- compatibility-matrix:end -->

## Behavior by area

| Area | saneyaml policy | libyaml / YAML 1.1 | yaml-rust2 / saphyr | serde_yaml |
|---|---|---|---|---|
| `on`, `off`, `yes`, `no` | Strings by default; booleans only under explicit YAML 1.1 | Often booleans | Per schema | Data-model dependent |
| Duplicate keys | Rejected after alias expansion (`1` and `"1"` are distinct keys) | Often last-wins | yaml-rust2 rejects some; saphyr accepts X38W | Rejects duplicate scalar keys |
| Merge key `<<` | Expanded by default in loaded trees and Serde reads; raw events and `LosslessStream` keep it literal; `Value::apply_merge()` is an explicit helper | Expanded, earlier merges win | Preserved literally | Literal in `Value` until `apply_merge()` |
| Anchors and aliases | Semantic trees clone acyclic aliases (no graph identity); `LosslessStream` keeps alias-to-anchor identity | Sometimes graph identity | Clone-on-alias | Accepted in read paths |
| Custom tags | Retained in `Value`/events/lossless; transparent for typed reads; `%TAG` handles resolved; undeclared handles rejected | Supported | Supported | Partial/lossy |
| Comments / formatting | Discarded by semantic loaders; retained by `LosslessStream` for byte-stable replay and edits | Not semantic | Not semantic | Discarded |
| Emission | `structural()` is deterministic default; `byte_compatible()` matches `serde_yaml` bytes for the documented corpus; document-marker policy matches `serde_yaml` | Manual comparison only | Manual comparison only | Marker policy matched; bytes for the supported corpus |
| Numbers / timestamps / binary | Decimals + underscores + special floats + `0123` (decimal) by default; octal/hex/binary/sexagesimal, `!!timestamp`, and `!!binary` under explicit YAML 1.1 or tags | Broad YAML 1.1 typing | Varies by crate | Data-model dependent |
| Directives | `%YAML` / `%TAG` accepted as syntax; `yaml_version_directive()` lets `%YAML 1.1` pick legacy construction | May affect schema | Exposed by parser | Usually not a value |
| Explicit core tags | `!!int`, `!!float`, `!!bool`, `!!null`, `!!str`, `!!timestamp`, `!!binary` preserved and coerced for typed reads (verbatim, canonical URI, or `%TAG` handle) | Common | Varies | Partial/lossy |
| YAML 1.1 collection/structural tags | `!!set`, `!!omap`, `!!pairs`, `!!seq`, `!!map`, `!!value` retained and mapped to typed targets; malformed payloads rejected with spans | Lossy recovery | Tag info available; contracts differ | Not retained |

## Scalar Resolution Modes

`Schema::Yaml12` is the retained spelling for `Schema::Core` (the default);
`Schema::Yaml11` is the retained spelling for `Schema::LegacySerdeYaml`.
`Schema::Json` resolves only JSON lowercase booleans/null and JSON numbers, then
keeps other scalar text as strings. `Schema::Failsafe` keeps every scalar a
string. Missing mapping values and empty documents are null in every mode.

| Plain scalar | Core / Yaml12 | Json | Failsafe | LegacySerdeYaml / Yaml11 |
|---|---|---|---|---|
| missing value | null | null | null | null |
| `~` | null | string | string | null |
| `null`, `Null`, `NULL` | null | `null` only is null; other spellings string | string | null |
| `true`, `false` | bool | bool | string | bool |
| `True`, `TRUE`, `False`, `FALSE` | bool | string | string | bool |
| `yes`, `no`, `on`, `off`, `y`, `n`, `NO` | string | string | string | bool |
| `123`, `+12`, `0123`, `1_000` | decimal number | JSON number only; `+12`, `0123`, and underscores string | string | number; `0123` is octal |
| `0x7B`, `0b1010`, `0o77` | string | string | string | hex and binary numbers; `0o77` string |
| `1:20`, `1:20:30.5` | string | string | string | sexagesimal number |
| `1.5` | float | JSON float | string | float |
| `.inf`, `-.Inf`, `.NAN` | float | string | string | float |
| `2026-05-24`, timestamp datetimes | string | string | string | retained `!!timestamp` string with `Timestamp` typed reads |

## Tree-shape divergences

A few YAML 1.2 inputs parse fine as events but yield **tree-shape divergences**
in the loaded tree, where reference loaders disagree. saneyaml keeps these in
event parity and shared-reference acceptance, excludes them from loaded-tree
value-shape parity, and pins a divergence record for each:

- **PW8X** and **6KGN** — anchors on **empty scalar nodes**.
- **S4JQ** — an **explicit non-specific tag** shape on an empty node.
- **C4HZ** — a **custom tag plus** a **schema scalar divergence**.
- **FH7J** — **tags on empty scalar** **nodes**.

`tests/parity_manifest.rs` gates these terms and the event/tree/shared-reference
ledgers; `cargo test --test conformance_dashboard -- --nocapture` prints the full
402-case selected-suite dashboard with divergence overlays.

## Event and streaming contracts

- `EventStream` is the stable pull-based parser-event surface; `parse_events` is
  the all-or-error collector over the same events. Events carry scalar style,
  block/flow collection style, tags, anchors, alias events, and `DocumentStart`
  directives. Aliases are **not** expanded here.
- `DocumentStream` is the semantic pull stream — one merge-expanded `Node` per
  document, same schema/limits/spans as `parse_documents`.
- Reader constructors fully buffer bounded input before yielding, so streaming
  bounds the retained parsed representation, not the source bytes.

Raw scalar spelling and graph identity are not exposed by events; recover them
with `LosslessStream::source_fragment(span)` and the lossless graph.

## Threat Model and Resource Guarantees

The defended input is untrusted YAML at every load entrypoint. With default
`LoadOptions`, the crate rejects:

- input above **64 MiB** before parsing,
- alias-expansion bombs (input-derived budget) and recursive aliases,
- nesting beyond **128**, scalars above **1 MiB**, and collections above
  **16,384** entries,

with span-bearing diagnostics. Raw event and lossless streams validate alias
references but don't expand them. Callers can tighten or relax each limit through
`LoadOptions`; a `without_*_limit()` opt-out transfers that bound to the caller.

These are **structural construction limits**, not wall-clock or resident-memory
guarantees: reader entrypoints fully buffer bounded input, and your own
`Deserialize` impls can allocate after the YAML layer hands them bounded values.
saneyaml validates YAML structure, not application schemas. See
[Untrusted input](untrusted-input.md) for the how-to and [SECURITY.md](../SECURITY.md)
for reporting.

## Public API stability

The crate is pre-1.0 (MSRV Rust 1.88), but the preview surface is SemVer-visible:
public exports, enum variants, struct fields, constants, and the
package-vs-library name split are commitments. [`docs/PUBLIC_API.txt`](PUBLIC_API.txt)
is the committed snapshot checked for drift; intentional changes must update it
along with this file and [MIGRATION.md](MIGRATION.md).

`saneyaml::Error` keeps a flat `Display` compatible with the preview contract and
exposes additive `category()`, `path()`, `document_index()`, and
`render_source(...)` diagnostics. `saneyaml::Index` and `saneyaml::mapping::Index`
are sealed.
