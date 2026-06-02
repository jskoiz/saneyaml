# Architecture

## Package and Library Names

The crates.io package name and the Rust library target are both `saneyaml`, so
downstream code imports this crate as `saneyaml::...`:

```toml
[dependencies]
saneyaml = "0.1"
```

For drop-in `serde_yaml` migration, Cargo dependency renaming keeps existing
source imports intact:

```toml
[dependencies]
serde_yaml = { package = "saneyaml", version = "0.1" }
```

or a local source alias:

```rust
use saneyaml as serde_yaml;
```

## Monolith Decision

`saneyaml` is one crate for the first public release. It is not a crate family
and does not publish separate `yaml-core`, `yaml-value`, `yaml-serde`, or
`yaml-edit` packages.

The parser, tree model, deserializer, emitter, and lossless source model are
tightly coupled today. In particular, `parse.rs` and `de.rs` share reader
ingestion, limits, schema construction, merge behavior, and span diagnostics.
Splitting those seams before real downstream adoption would create compatibility
and versioning surfaces without reducing meaningful user complexity.

The monolith keeps one SemVer contract, one feature map, one diagnostics model,
and one package alias story for `serde_yaml` migration.

## Feature Facade

The default feature set is:

```toml
default = ["serde", "emit", "lossless"]
```

Feature intent:

- `serde`: the default Serde read/write facade and migration surface.
- `emit`: structural writer and serializer-facing emission controls.
- `lossless`: source-backed graph inspection and format-preserving edit helpers.

The `lossless` surface is feature-gated and enabled by default. The `serde` and
`emit` features are named facade features for the 0.1 line; they remain enabled
by default because Serde integration and structural emission are part of the
first package contract. A future non-default build can narrow those surfaces
only with explicit tests and documentation.

## Stability Boundary

The crate is pre-1.0. Public exports, public enum variants, public struct
fields, feature names, package metadata, MSRV, and the package-vs-library name
split are still SemVer-visible for adopter trust. Intentional changes to those
surfaces must update `PUBLIC_API.txt`, `MIGRATION.md`, `COMPATIBILITY.md`, and
the baseline evidence rather than relying on silent drift.
