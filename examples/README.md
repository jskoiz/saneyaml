# Examples

This directory is development-only. Most of these examples are not shipped to
crates.io. The published crate's `include` list (see `Cargo.toml`) ships exactly
one example, `serde_yaml_migration.rs`; everything else here is for local
development, benchmarking, and conformance checks against the source tree.

| example | shipped to crates.io | purpose |
|---|:---:|---|
| `serde_yaml_migration.rs` | yes | Migrating `serde_yaml` call sites to `saneyaml` via a drop-in alias. |
| `real_world_benchmark.rs` | no | Real-world config corpus throughput benchmark. |
| `large_input_benchmark.rs` | no | Large-input throughput and retained-output benchmark. |
| `dhat_memory.rs` | no | Allocator-backed (dhat) memory measurement. |
| `conformance_compare.rs` | no | Head-to-head YAML test-suite conformance comparison. |

The benchmark and conformance examples depend on dev-dependencies (for example
`serde_yaml`, `saphyr`, `yaml-rust2`, `dhat`) and on in-repo fixtures that are
not part of the published package, so they only build from a checkout of this
repository. See `docs/BENCHMARKS.md` for the exact commands and pinned
reference-crate versions used to capture the published numbers.
