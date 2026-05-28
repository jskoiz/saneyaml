# Real-World Config Benchmarks

The benchmark command parses the checked-in real-world fixture registry without
timing file I/O. It reports aggregate cost across all fixtures so small files do
not dominate the signal.

```sh
cargo run --release --example real_world_benchmark
```

Environment:

- Workspace: `/Users/jk/Desktop/yaml`
- Fixture set: 26 files / 32 YAML documents / 19,450 bytes
- Iterations: controlled by `YAML_BENCH_ITERS`, default `200`

Latest captured run: 2026-05-28 on the local `release` profile with
`YAML_BENCH_ITERS` unset, so the default 200 iterations were used.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 200 | 19,450 | 32 | 102.814 | 26.43 |
| `yaml::from_documents_str::<Value>` | 200 | 19,450 | 32 | 156.556 | 40.25 |
| `serde_yaml::Value` stream | 200 | 19,450 | 32 | 103.176 | 26.52 |
| `yaml_rust2::YamlLoader` | 200 | 19,450 | 32 | 86.539 | 22.25 |
| `saphyr::Yaml::load_from_str` | 200 | 19,450 | 32 | 78.637 | 20.22 |

Interpretation: this crate is in the same order of magnitude as the current
Serde migration baseline for the selected config fixtures. `yaml-rust2` and
`saphyr` remain faster on this small aggregate parser/load benchmark, so
performance is credible for preview evaluation but not yet a competitive
headline.
