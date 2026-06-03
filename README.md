# saneyaml

[![CI](https://github.com/jskoiz/saneyaml/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jskoiz/saneyaml/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-green)](LICENSE.md)
[![rust](https://img.shields.io/badge/rust-1.88%2B-blue)](Cargo.toml)
[![unsafe](https://img.shields.io/badge/unsafe-forbidden-success)](src/lib.rs)

Serde-first YAML for Rust. `saneyaml` reads and writes common config YAML with
**YAML 1.2 by default** (so `NO` stays the string `"NO"`, not `false`),
diagnostics, and resource limits. Pure Rust, `#![forbid(unsafe_code)]`.

## Install

```toml
[dependencies]
saneyaml = "0.1"
```

Then use it:

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct Config {
    name: String,
    port: u16,
}

fn main() -> Result<(), saneyaml::Error> {
    let cfg: Config = saneyaml::from_str("name: web\nport: 8080\n")?;
    assert_eq!(cfg.port, 8080);

    let text = saneyaml::to_string(&cfg)?;
    println!("{text}");
    Ok(())
}
```

Coming from the archived `serde_yaml`? It's close to a drop-in — see
[MIGRATION.md](docs/MIGRATION.md).

## Why saneyaml

- **YAML 1.2 by default** — no "Norway problem": `NO`/`on`/`off`/`yes` stay
  strings. Opt into YAML 1.1 / `serde_yaml`-style resolution explicitly via
  schema modes (`Core`, `Json`, `Failsafe`, `LegacySerdeYaml`).
- **Serde-first** — `from_str` / `from_slice` / `from_reader`, `to_string` /
  `to_writer`, and a `serde_yaml`-style `Value`.
- **Diagnostics** — line/column, in-document key path (e.g. `server.port`),
  and opt-in source-caret rendering.
- **Resource limits** — unsafe-free, with input-size, alias-expansion,
  nesting-depth, scalar-length, and collection-size limits.
- **Streaming and lossless editing** — pull-based streaming (`EventStream` /
  `DocumentStream`) and a lossless, comment-preserving editor.
- **Benchmarked** — on the config benchmark corpus it parses faster than `yaml-rust2`
  and `saphyr`; see [BENCHMARKS.md](docs/BENCHMARKS.md).

## Status

Pre-1.0 (`0.1.0`), MSRV Rust 1.88. The public API is a preview surface but is
treated as SemVer-visible: breaking changes and MSRV bumps are explicit release
decisions.

## Documentation

- [MIGRATION.md](docs/MIGRATION.md) — `serde_yaml` migration cookbook + support matrix
- [COMPATIBILITY.md](docs/COMPATIBILITY.md) — schema modes, scalar resolution, divergences, threat model
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — crate layout and design
- [BENCHMARKS.md](docs/BENCHMARKS.md) · [SECURITY.md](SECURITY.md) · [CONTRIBUTING.md](CONTRIBUTING.md) · [CHANGELOG.md](CHANGELOG.md)

## License

MIT — see [LICENSE.md](LICENSE.md).
