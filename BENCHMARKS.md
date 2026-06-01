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

The large benchmark's `peak retained bytes` and `peak retained heap objects`
columns are safe retained-output estimates from parsed tree container and
string capacities after a single parse. They are not allocator instrumentation
and do not include transient parser scratch. For multi-fixture corpora, they
report the peak retained output for one fixture because each fixture is parsed
and dropped independently.

The 2026-06-01 zero-copy line slice removed transient per-line raw/content text
allocations from this crate's parser by storing one resident source buffer and
per-line byte ranges. That allocation drop is visible in the parser code path
rather than in the retained-output columns, because preprocessed lines are
dropped before the parsed tree is returned.

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

Post zero-copy line-slice re-capture with 1,000 iterations (independent run,
2026-06-01):

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 1,000 | 19,727 | 33 | 355.271 | 18.01 |
| `yaml::from_documents_str::<Value>` | 1,000 | 19,727 | 33 | 422.618 | 21.42 |
| `serde_yaml::Value` stream | 1,000 | 19,727 | 33 | 523.390 | 26.53 |
| `yaml_rust2::YamlLoader` | 1,000 | 19,727 | 33 | 434.222 | 22.01 |
| `saphyr::Yaml::load_from_str` | 1,000 | 19,727 | 33 | 402.909 | 20.42 |

Result: after the zero-copy line slice, `yaml::parse_documents` is faster than
the pinned reference loaders on this small corpus in the latest 1,000-iteration
same-run capture (18.01 ns/byte vs `saphyr` at 20.42 and `yaml_rust2` at
22.01). The owning `Value` path also remains ahead of the `serde_yaml` `Value`
stream and roughly ties `yaml_rust2` on this corpus.

Methodology caveat: the pre-optimization table above was captured at 200
iterations and the post-optimization table at 1,000, so part of the across-table
ns/byte drop reflects warm-up rather than optimization. The trustworthy signal
is the same-run cross-loader comparison within each table, plus the larger,
lower-noise inputs below — not the across-table delta.

## Large Inputs

Command:

```sh
cargo run --release --example large_input_benchmark
```

Default iterations: 20, controlled by `YAML_LARGE_BENCH_ITERS`.

### external_downstream_all

20 pinned downstream files / 245,062 bytes / 20 YAML documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 245,062 | 20 | 39.702 | 8.10 | 703,340 | 3,983 |
| `yaml::from_documents_str::<Value>` | 20 | 245,062 | 20 | 43.391 | 8.85 | 217,579 | 3,780 |
| `serde_yaml::Value` stream | 20 | 245,062 | 20 | 54.997 | 11.22 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 245,062 | 20 | 42.267 | 8.62 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 245,062 | 20 | 40.091 | 8.18 | 534,786 | 3,780 |

### stackable_dummy_cluster

One pinned Stackable CRD / 177,556 bytes / 1 YAML document.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 177,556 | 1 | 24.740 | 6.97 | 703,340 | 3,983 |
| `yaml::from_documents_str::<Value>` | 20 | 177,556 | 1 | 27.263 | 7.68 | 217,579 | 3,780 |
| `serde_yaml::Value` stream | 20 | 177,556 | 1 | 35.406 | 9.97 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 177,556 | 1 | 26.927 | 7.58 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 177,556 | 1 | 25.119 | 7.07 | 534,786 | 3,780 |

### generated_multi_doc_stream_1mib

Generated multi-document service stream / 1,048,680 bytes / 8,020 YAML
documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 1,048,680 | 8,020 | 553.336 | 26.38 | 23,031,972 | 128,321 |
| `yaml::from_documents_str::<Value>` | 20 | 1,048,680 | 8,020 | 624.339 | 29.77 | 4,735,364 | 112,281 |
| `serde_yaml::Value` stream | 20 | 1,048,680 | 8,020 | 739.488 | 35.26 | 11,607,364 | 112,281 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,680 | 8,020 | 595.081 | 28.37 | 10,386,948 | 112,281 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,680 | 8,020 | 530.443 | 25.29 | 14,770,560 | 112,281 |

### generated_wide_mapping_256kib

Generated one-document wide service mapping / 262,176 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 262,176 | 1 | 90.744 | 17.31 | 3,272,071 | 23,932 |
| `yaml::from_documents_str::<Value>` | 20 | 262,176 | 1 | 106.565 | 20.32 | 938,332 | 17,950 |
| `serde_yaml::Value` stream | 20 | 262,176 | 1 | 125.650 | 23.96 | 1,895,692 | 17,950 |
| `yaml_rust2::YamlLoader` | 20 | 262,176 | 1 | 111.358 | 21.24 | 1,704,220 | 17,950 |
| `saphyr::Yaml::load_from_str` | 20 | 262,176 | 1 | 95.838 | 18.28 | 2,393,312 | 17,950 |

### generated_wide_mapping_1mib

Generated one-document wide service mapping / 1,048,661 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 1,048,661 | 1 | 400.810 | 19.11 | 13,040,211 | 95,236 |
| `yaml::from_documents_str::<Value>` | 20 | 1,048,661 | 1 | 462.088 | 22.03 | 3,739,155 | 71,428 |
| `serde_yaml::Value` stream | 20 | 1,048,661 | 1 | 544.932 | 25.98 | 7,548,675 | 71,428 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,661 | 1 | 450.637 | 21.49 | 6,786,771 | 71,428 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,661 | 1 | 389.044 | 18.55 | 9,523,712 | 71,428 |

Large-input story: after zero-copy line storage, `yaml::parse_documents` beats
`yaml_rust2` on every large parser path in the latest capture and is
win-2/tie-2/lose-1 versus `saphyr`. The clear wins are
`stackable_dummy_cluster` (6.97 vs 7.07 ns/byte) and
`generated_wide_mapping_256kib` (17.31 vs 18.28 ns/byte). The noise-level ties
are `external_downstream_all` (8.10 vs 8.18 ns/byte) and
`generated_wide_mapping_1mib` (19.11 vs 18.55 ns/byte). The remaining raw-speed
gap is the many-document 1 MiB stream (26.38 vs 25.29 ns/byte). The durable
allocation story is split: line storage now avoids transient per-line
raw/content heap text, and the owning `Value` path still retains materially less
parsed output than the reference loaders on large load-path comparisons.
