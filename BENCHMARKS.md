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

The 2026-06-01 no-merge fast path records whether each parsed document contains
a semantic merge key and skips the post-parse merge traversal when none was
seen. In the same-session target capture, `generated_multi_doc_stream_1mib`
`yaml::parse_documents` moved from 25.87 to 23.98 ns/byte while `saphyr` moved
from 24.86 to 24.89 ns/byte. Retained output estimates are unchanged because
the returned tree shape is unchanged; the removed work is transient
per-document merge scanning and its scratch stack.

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

Post no-merge fast-path re-capture with the default 200 iterations
(independent run, 2026-06-01):

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 200 | 19,727 | 33 | 72.136 | 18.28 |
| `yaml::from_documents_str::<Value>` | 200 | 19,727 | 33 | 84.028 | 21.30 |
| `serde_yaml::Value` stream | 200 | 19,727 | 33 | 103.945 | 26.35 |
| `yaml_rust2::YamlLoader` | 200 | 19,727 | 33 | 85.682 | 21.72 |
| `saphyr::Yaml::load_from_str` | 200 | 19,727 | 33 | 79.723 | 20.21 |

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
| `yaml::parse_documents` | 20 | 245,062 | 20 | 39.130 | 7.98 | 703,340 | 3,983 |
| `yaml::from_documents_str::<Value>` | 20 | 245,062 | 20 | 44.597 | 9.10 | 217,579 | 3,780 |
| `serde_yaml::Value` stream | 20 | 245,062 | 20 | 55.035 | 11.23 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 245,062 | 20 | 42.009 | 8.57 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 245,062 | 20 | 40.579 | 8.28 | 534,786 | 3,780 |

### stackable_dummy_cluster

One pinned Stackable CRD / 177,556 bytes / 1 YAML document.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 177,556 | 1 | 24.375 | 6.86 | 703,340 | 3,983 |
| `yaml::from_documents_str::<Value>` | 20 | 177,556 | 1 | 27.735 | 7.81 | 217,579 | 3,780 |
| `serde_yaml::Value` stream | 20 | 177,556 | 1 | 35.637 | 10.04 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 177,556 | 1 | 26.718 | 7.52 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 177,556 | 1 | 25.821 | 7.27 | 534,786 | 3,780 |

### generated_multi_doc_stream_1mib

Generated multi-document service stream / 1,048,680 bytes / 8,020 YAML
documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 1,048,680 | 8,020 | 502.904 | 23.98 | 23,031,972 | 128,321 |
| `yaml::from_documents_str::<Value>` | 20 | 1,048,680 | 8,020 | 571.076 | 27.23 | 4,735,364 | 112,281 |
| `serde_yaml::Value` stream | 20 | 1,048,680 | 8,020 | 730.933 | 34.85 | 11,607,364 | 112,281 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,680 | 8,020 | 561.270 | 26.76 | 10,386,948 | 112,281 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,680 | 8,020 | 521.988 | 24.89 | 14,770,560 | 112,281 |

### generated_wide_mapping_256kib

Generated one-document wide service mapping / 262,176 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 262,176 | 1 | 91.745 | 17.50 | 3,272,071 | 23,932 |
| `yaml::from_documents_str::<Value>` | 20 | 262,176 | 1 | 105.599 | 20.14 | 938,332 | 17,950 |
| `serde_yaml::Value` stream | 20 | 262,176 | 1 | 123.969 | 23.64 | 1,895,692 | 17,950 |
| `yaml_rust2::YamlLoader` | 20 | 262,176 | 1 | 104.617 | 19.95 | 1,704,220 | 17,950 |
| `saphyr::Yaml::load_from_str` | 20 | 262,176 | 1 | 94.489 | 18.02 | 2,393,312 | 17,950 |

### generated_wide_mapping_1mib

Generated one-document wide service mapping / 1,048,661 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 1,048,661 | 1 | 374.298 | 17.85 | 13,040,211 | 95,236 |
| `yaml::from_documents_str::<Value>` | 20 | 1,048,661 | 1 | 434.528 | 20.72 | 3,739,155 | 71,428 |
| `serde_yaml::Value` stream | 20 | 1,048,661 | 1 | 528.179 | 25.18 | 7,548,675 | 71,428 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,661 | 1 | 433.680 | 20.68 | 6,786,771 | 71,428 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,661 | 1 | 384.807 | 18.35 | 9,523,712 | 71,428 |

Large-input story: after zero-copy line storage and the no-merge fast path,
`yaml::parse_documents` beats `yaml_rust2` on every large parser path in the
latest capture and closes the prior many-document `saphyr` gap
(23.98 vs 24.89 ns/byte). The same-run parser rows are 5-of-5 faster than
`saphyr`, though `external_downstream_all` and `generated_wide_mapping_1mib`
remain close enough to treat as noise-level wins until repeated samples quantify
variance. The durable allocation story is split: line storage avoids transient
per-line raw/content heap text, the no-merge fast path avoids transient
per-document merge traversal for documents without semantic merge keys, and the
owning `Value` path still retains materially less parsed output than the
reference loaders on large load-path comparisons.
