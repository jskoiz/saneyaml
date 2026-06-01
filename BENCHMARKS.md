# Real-World Config Benchmarks And Large Inputs

The benchmark examples parse checked-in or generated YAML without timing file
I/O. They report aggregate cost so small files do not dominate the signal.

```sh
cargo run --release --example real_world_benchmark
YAML_BENCH_ITERS=1000 cargo run --release --example real_world_benchmark
cargo run --release --example large_input_benchmark
YAML_LARGE_BENCH_ITERS=20 cargo run --release --example large_input_benchmark
```

Environment for the latest captured run:

- Workspace: `/Users/jk/Desktop/yaml`
- Reference crates: `yaml-rust2 0.11.0`, `saphyr 0.0.6`
- Small fixture set: 27 files / 33 YAML documents / 19,727 bytes
- Large fixture set: pinned downstream fixtures plus generated 1 MiB inputs
- Captured: 2026-06-01 on the local `release` profile

The large benchmark's `peak retained bytes` column is a safe retained-output
estimate from parsed tree container and string capacities after a single parse.
It is not allocator instrumentation and does not include source input storage.
For multi-fixture corpora, it reports the peak retained output for one fixture
because each fixture is parsed and dropped independently.

## Real-World Config Corpus

Same-turn pre-optimization baseline, captured before this milestone with the
default 200 iterations:

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 200 | 19,727 | 33 | 126.377 | 32.03 |
| `yaml::from_documents_str::<Value>` | 200 | 19,727 | 33 | 141.211 | 35.79 |
| `serde_yaml::Value` stream | 200 | 19,727 | 33 | 153.465 | 38.90 |
| `yaml_rust2::YamlLoader` | 200 | 19,727 | 33 | 100.959 | 25.59 |
| `saphyr::Yaml::load_from_str` | 200 | 19,727 | 33 | 92.393 | 23.42 |

Post-optimization capture with 1,000 iterations:

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 1,000 | 19,727 | 33 | 431.262 | 21.86 |
| `yaml::from_documents_str::<Value>` | 1,000 | 19,727 | 33 | 506.879 | 25.69 |
| `serde_yaml::Value` stream | 1,000 | 19,727 | 33 | 564.256 | 28.60 |
| `yaml_rust2::YamlLoader` | 1,000 | 19,727 | 33 | 454.845 | 23.06 |
| `saphyr::Yaml::load_from_str` | 1,000 | 19,727 | 33 | 453.165 | 22.97 |

Result: `yaml::parse_documents` moved from 32.03 ns/byte to 21.86 ns/byte on
the existing config corpus and is faster than both pinned reference loaders in
the latest 1,000-iteration capture. `Value` loading also moved ahead of
`serde_yaml` on this corpus.

## Large Inputs

Command:

```sh
cargo run --release --example large_input_benchmark
```

Default iterations: 20, controlled by `YAML_LARGE_BENCH_ITERS`.

### external_downstream_all

20 pinned downstream files / 245,062 bytes / 20 YAML documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes |
|---|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 245,062 | 20 | 46.926 | 9.57 | 703,340 |
| `yaml::from_documents_str::<Value>` | 20 | 245,062 | 20 | 52.384 | 10.69 | 217,579 |
| `serde_yaml::Value` stream | 20 | 245,062 | 20 | 55.084 | 11.24 | 396,987 |
| `yaml_rust2::YamlLoader` | 20 | 245,062 | 20 | 43.325 | 8.84 | 382,497 |
| `saphyr::Yaml::load_from_str` | 20 | 245,062 | 20 | 40.399 | 8.24 | 534,786 |

### stackable_dummy_cluster

One pinned Stackable CRD / 177,556 bytes / 1 YAML document.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes |
|---|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 177,556 | 1 | 28.812 | 8.11 | 703,340 |
| `yaml::from_documents_str::<Value>` | 20 | 177,556 | 1 | 31.887 | 8.98 | 217,579 |
| `serde_yaml::Value` stream | 20 | 177,556 | 1 | 35.400 | 9.97 | 396,987 |
| `yaml_rust2::YamlLoader` | 20 | 177,556 | 1 | 27.440 | 7.73 | 382,497 |
| `saphyr::Yaml::load_from_str` | 20 | 177,556 | 1 | 25.157 | 7.08 | 534,786 |

### generated_multi_doc_stream_1mib

Generated multi-document service stream / 1,048,680 bytes / 8,020 YAML
documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes |
|---|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 1,048,680 | 8,020 | 613.773 | 29.26 | 23,031,972 |
| `yaml::from_documents_str::<Value>` | 20 | 1,048,680 | 8,020 | 688.037 | 32.80 | 4,735,364 |
| `serde_yaml::Value` stream | 20 | 1,048,680 | 8,020 | 705.486 | 33.64 | 11,607,364 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,680 | 8,020 | 591.445 | 28.20 | 10,386,948 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,680 | 8,020 | 536.775 | 25.59 | 14,770,560 |

### generated_wide_mapping_256kib

Generated one-document wide service mapping / 262,176 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes |
|---|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 262,176 | 1 | 106.831 | 20.37 | 3,272,071 |
| `yaml::from_documents_str::<Value>` | 20 | 262,176 | 1 | 118.994 | 22.69 | 938,332 |
| `serde_yaml::Value` stream | 20 | 262,176 | 1 | 124.887 | 23.82 | 1,895,692 |
| `yaml_rust2::YamlLoader` | 20 | 262,176 | 1 | 108.355 | 20.66 | 1,704,220 |
| `saphyr::Yaml::load_from_str` | 20 | 262,176 | 1 | 96.875 | 18.48 | 2,393,312 |

### generated_wide_mapping_1mib

Generated one-document wide service mapping / 1,048,661 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes |
|---|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 1,048,661 | 1 | 446.672 | 21.30 | 13,040,211 |
| `yaml::from_documents_str::<Value>` | 20 | 1,048,661 | 1 | 509.926 | 24.31 | 3,739,155 |
| `serde_yaml::Value` stream | 20 | 1,048,661 | 1 | 548.149 | 26.14 | 7,548,675 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,661 | 1 | 492.104 | 23.46 | 6,786,771 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,661 | 1 | 426.032 | 20.31 | 9,523,712 |

Large-input story: the parser is competitive on pinned downstream real-world
YAML and beats `yaml-rust2` on the 1 MiB generated wide-mapping path. The
Serde-facing `Value` path now avoids quadratic map construction on wide
mappings, beats `serde_yaml` on the large real corpus and generated wide maps,
and retains materially less parsed output than the reference loaders on those
load-path comparisons. `saphyr` remains the fastest large-input parser/load
reference, especially for many-document streams, so future work should focus on
reducing document-vector and per-line parser retention if that path becomes the
next performance target.
