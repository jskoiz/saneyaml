# Migrating from serde_yaml

`serde_yaml` is archived. For config-shaped Serde code, saneyaml is close to a
drop-in: the read API, `Value`, and the `with::singleton_map` helpers keep the
same spelling — now with YAML 1.2 scalar resolution and richer diagnostics.

This page is the call-site cookbook. The exhaustive support matrix, divergences,
and threat model live in [COMPATIBILITY.md](COMPATIBILITY.md); the scalar-typing
differences are in [Schema modes](schema-modes.md).

> **Scope.** saneyaml is an adoption candidate for config-shaped Serde reads plus
> structural writes. It is *not* a blanket drop-in for every YAML document, every
> emitter byte, or full YAML 1.1 / libyaml behavior.

## Two ways to switch

**Keep `serde_yaml::…` spellings** — alias the package in Cargo, change nothing
in source:

```toml
[dependencies]
serde_yaml = { package = "saneyaml", version = "0.3.0" }
```

`serde_yaml::from_str`, `serde_yaml::Value`, `serde_yaml::with::singleton_map`,
and friends keep compiling against this crate.

**Or import directly** and rewrite the prefix:

```toml
[dependencies]
saneyaml = "0.3.0"
```

```rust
// mechanical rename
let cfg: saneyaml::Value = saneyaml::from_str(input)?;

// …or alias one file at a time
use saneyaml as serde_yaml;
let cfg: serde_yaml::Value = serde_yaml::from_str(input)?;
```

The shipped [`examples/serde_yaml_migration.rs`](../examples/serde_yaml_migration.rs)
compiles the full alias surface end to end.

## Cookbook

Each recipe shows the call site. Under a Cargo/source alias, keep the
`serde_yaml::` spelling; with a direct import, swap the prefix to `saneyaml::`.

**Typed reads** — unchanged:

```rust
let config: Config = saneyaml::from_str(input)?;
let config: Config = saneyaml::from_slice(bytes)?;
let config: Config = saneyaml::from_reader(reader)?;
```

**Value indexing and patching** — unchanged:

```rust
let mut value: saneyaml::Value = saneyaml::from_str(input)?;
value["services"]["api"]["image"] = saneyaml::Value::from("nginx:latest");
let ports = value["services"]["api"]["ports"].as_sequence();
```

**Tagged enums / singleton maps** — same helpers:

```rust
#[derive(serde::Serialize, serde::Deserialize)]
struct Job {
    #[serde(with = "saneyaml::with::singleton_map")]
    action: Action,
}
```

Use `singleton_map_recursive` for nested enum payloads.

**Multi-document streams** — iterate, or collect:

```rust
let docs = saneyaml::Deserializer::from_str(stream)
    .map(Config::deserialize)
    .collect::<Result<Vec<_>, _>>()?;

let docs: Vec<Config> = saneyaml::from_documents_str(stream)?; // additive convenience
```

**Structural writes** — unchanged:

```rust
let text = saneyaml::to_string(&config)?;
saneyaml::to_writer(&mut writer, &config)?;
```

**Errors** — `line()` / `column()` still work; `span()`, `category()`, `path()`,
and `render_source()` are additive ([Diagnostics](diagnostics.md)):

```rust
let err = saneyaml::from_str::<Config>("name: [").unwrap_err();
if let Some(loc) = err.location() {
    eprintln!("{}:{}", loc.line(), loc.column());
}
```

## What behaves differently

Five things a migrator should know — most code never touches them:

| Change | What it means for you |
|---|---|
| **YAML 1.2 by default** | `no` / `on` / `NO` stay strings, not booleans. Opt into the old behavior per call with `LoadOptions::legacy_serde_yaml()`. See [Schema modes](schema-modes.md). |
| **Merge keys expand by default** | Loaded `Node`/`Value` give you the merged result. `serde_yaml::Value` kept the literal `<<` until `apply_merge()`. To see raw `<<`, use events or the [lossless graph](editing.md). |
| **`Value` is spanless** | It won't coerce a number/bool into a `String` target, and it doesn't carry comments, anchors, or graph identity. Read with `from_str`/`from_node` when source text matters; use the lossless graph for formatting. |
| **Structural writer** | `to_string` emits clean deterministic YAML, not byte-identical `serde_yaml`. Pass `EmitOptions::byte_compatible()` for the supported byte corpus. |
| **Resource limits on by default** | Untrusted input is bounded (64 MiB, depth, scalar, collection, alias). Tune or opt out via `LoadOptions`. See [Untrusted input](untrusted-input.md). |

## Support matrix

All of the following resolve under both rename paths and are covered by the swap
harness and downstream smokes:

| `serde_yaml` surface | Status |
|---|---|
| `from_str` / `from_slice` / `from_reader` | Covered for typed config reads and `Value` |
| `Deserializer::{from_str, from_slice, from_reader}` | Covered, incl. multi-document iteration |
| `Value` / `Mapping` / `Number` | Covered: reads, mutation, indexing, helpers, traits |
| `value::{to_value, Serializer}` | Covered for config-shaped serialization |
| `to_string` / `to_writer` / `Serializer` | Structural output covered; `byte_compatible()` matches bytes on the supported corpus |
| `with::singleton_map` / `singleton_map_recursive` | Covered for read and write |
| `Error` / `Result` / `Location` | Covered; richer diagnostics are additive |

The indexing traits (`Index`, `mapping::Index`) are sealed, as they were
upstream — use the built-in string / `usize` / `Value` lookups.

## Proof

The migration claims are executable, not aspirational:

- `tests/serde_yaml_swap_harness.rs` — the same call sites run against
  `serde_yaml 0.9.34` and against this crate under the `serde_yaml` dependency
  name.
- `tests/downstream_migration_harness.rs` and
  `tests/external_downstream_migration.rs` — pinned real-world configs and
  reduced fixtures from real `serde_yaml` users (Pingora, rust-i18n, cfn-guard,
  navi, Stackable).
- `scripts/downstream-build-trials.sh` — packages this crate and builds those
  downstreams with their `serde_yaml` dependency rewritten to it.

```sh
cargo test --test serde_yaml_swap_harness --test downstream_migration_harness
cargo test --test external_downstream_migration
scripts/downstream-build-trials.sh smoke-only
```

Real-world gates currently cover 33 files / 39 documents across GitHub Actions,
Docker Compose, Kubernetes, Helm, OpenAPI, Wrangler, Ansible, CloudFormation/SAM,
Symfony, GitLab CI, CircleCI, and Azure Pipelines. They prove the selected
corpus — not a substitute for testing your own YAML.

## Migration impact ledger

| Area | Migration impact |
|---|---|
| Default merge expansion | Loaded `Node`/`Value` and Serde reads expand untagged and explicit merge-tag `<<` by default. Code that inspected merge syntax should switch to `parse_events` or `LosslessStream`. Explicit `!!str <<` and custom-tagged `<<` stay literal. |
| YAML 1.1 compatibility | Legacy scalar/merge behavior is opt-in via schema modes; default entrypoints stay YAML 1.2-oriented, so corpora that need 1.1 typing need opt-in tests. |
| Alias graph identity | Semantic trees clone acyclic aliases and reject recursion; graph-sensitive callers should use `LosslessStream`. |
| Lossless formatting | Comments, anchors, directives, and source style are preserved only by `LosslessStream` / `ConfigEditor`, not the semantic `Value` tree. |
| Parser acceptance differences | Some YAML 1.2 inputs libyaml rejects are accepted, and some malformed libyaml-tolerated inputs are rejected. Per-case detail lives in the divergence records. |
| Package status | `Cargo.toml` declares `saneyaml` 0.3.0 under the MIT license. |

## Known follow-up

- Keep the named external crate build trials current before broadening ecosystem
  replacement claims.
- Keep divergence records and migration-impact wording current as behavior
  changes.
- Treat full YAML compatibility and arbitrary source-preserving emission as
  future work until they are fixture-backed.
