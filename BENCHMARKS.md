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

The 2026-06-01 plain-scalar continuation slice delays `String` allocation until
a plain scalar is proven to span multiple lines. In the next same-session target
capture, `generated_multi_doc_stream_1mib` `yaml::parse_documents` moved from
23.98 to 21.87 ns/byte while `saphyr` measured 24.42 ns/byte. Retained output
estimates are unchanged because single-line scalar output is identical; the
removed work is transient short `String` allocation before the scalar falls back
to the inline parse path.

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

Post no-merge and plain-scalar fast-path re-capture with the default 200 iterations
(independent run, 2026-06-01):

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 200 | 19,727 | 33 | 70.310 | 17.82 |
| `yaml::from_documents_str::<Value>` | 200 | 19,727 | 33 | 80.490 | 20.40 |
| `serde_yaml::Value` stream | 200 | 19,727 | 33 | 102.217 | 25.91 |
| `yaml_rust2::YamlLoader` | 200 | 19,727 | 33 | 83.610 | 21.19 |
| `saphyr::Yaml::load_from_str` | 200 | 19,727 | 33 | 77.720 | 19.70 |

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
| `yaml::parse_documents` | 20 | 245,062 | 20 | 37.430 | 7.64 | 703,340 | 3,983 |
| `yaml::from_documents_str::<Value>` | 20 | 245,062 | 20 | 42.274 | 8.63 | 217,579 | 3,780 |
| `serde_yaml::Value` stream | 20 | 245,062 | 20 | 54.976 | 11.22 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 245,062 | 20 | 41.420 | 8.45 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 245,062 | 20 | 38.974 | 7.95 | 534,786 | 3,780 |

### stackable_dummy_cluster

One pinned Stackable CRD / 177,556 bytes / 1 YAML document.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 177,556 | 1 | 23.499 | 6.62 | 703,340 | 3,983 |
| `yaml::from_documents_str::<Value>` | 20 | 177,556 | 1 | 25.621 | 7.21 | 217,579 | 3,780 |
| `serde_yaml::Value` stream | 20 | 177,556 | 1 | 35.163 | 9.90 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 177,556 | 1 | 26.216 | 7.38 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 177,556 | 1 | 25.069 | 7.06 | 534,786 | 3,780 |

### generated_multi_doc_stream_1mib

Generated multi-document service stream / 1,048,680 bytes / 8,020 YAML
documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 1,048,680 | 8,020 | 458.747 | 21.87 | 23,031,972 | 128,321 |
| `yaml::from_documents_str::<Value>` | 20 | 1,048,680 | 8,020 | 530.467 | 25.29 | 4,735,364 | 112,281 |
| `serde_yaml::Value` stream | 20 | 1,048,680 | 8,020 | 720.784 | 34.37 | 11,607,364 | 112,281 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,680 | 8,020 | 565.036 | 26.94 | 10,386,948 | 112,281 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,680 | 8,020 | 512.208 | 24.42 | 14,770,560 | 112,281 |

### generated_wide_mapping_256kib

Generated one-document wide service mapping / 262,176 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 262,176 | 1 | 85.692 | 16.34 | 3,272,071 | 23,932 |
| `yaml::from_documents_str::<Value>` | 20 | 262,176 | 1 | 98.199 | 18.73 | 938,332 | 17,950 |
| `serde_yaml::Value` stream | 20 | 262,176 | 1 | 124.974 | 23.83 | 1,895,692 | 17,950 |
| `yaml_rust2::YamlLoader` | 20 | 262,176 | 1 | 107.656 | 20.53 | 1,704,220 | 17,950 |
| `saphyr::Yaml::load_from_str` | 20 | 262,176 | 1 | 93.863 | 17.90 | 2,393,312 | 17,950 |

### generated_wide_mapping_1mib

Generated one-document wide service mapping / 1,048,661 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `yaml::parse_documents` | 20 | 1,048,661 | 1 | 342.532 | 16.33 | 13,040,211 | 95,236 |
| `yaml::from_documents_str::<Value>` | 20 | 1,048,661 | 1 | 403.599 | 19.24 | 3,739,155 | 71,428 |
| `serde_yaml::Value` stream | 20 | 1,048,661 | 1 | 520.812 | 24.83 | 7,548,675 | 71,428 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,661 | 1 | 421.095 | 20.08 | 6,786,771 | 71,428 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,661 | 1 | 372.352 | 17.75 | 9,523,712 | 71,428 |

Large-input story: after zero-copy line storage, the no-merge fast path, and
delayed plain-scalar continuation allocation, `yaml::parse_documents` beats
`yaml_rust2` on every large parser path in the latest capture and closes the
prior many-document `saphyr` gap decisively (21.87 vs 24.42 ns/byte). The
same-run parser rows are 5-of-5 faster than `saphyr`, though
`external_downstream_all` remains close enough to treat as a noise-level win
until repeated samples quantify variance. The durable allocation story is
split: line storage avoids transient per-line raw/content heap text, the
no-merge fast path avoids transient per-document merge traversal for documents
without semantic merge keys, delayed plain-scalar allocation avoids short-lived
strings on single-line plain scalars, and the owning `Value` path still retains
materially less parsed output than the reference loaders on large load-path
comparisons.
