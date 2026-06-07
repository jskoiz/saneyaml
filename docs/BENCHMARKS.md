# Real-World Config Benchmarks And Large Inputs

The benchmark examples parse checked-in or generated YAML without timing file
I/O. They report aggregate cost so small files do not dominate the signal.
These benchmark and conformance commands are source-checkout-only: the published
crate package ships this document, but it intentionally excludes the
dev-dependency examples and fixture corpora used to regenerate the tables.

```sh
cargo run --locked --release --example real_world_benchmark
YAML_BENCH_ITERS=1000 cargo run --locked --release --example real_world_benchmark
cargo run --locked --release --example large_input_benchmark
YAML_LARGE_BENCH_ITERS=20 cargo run --locked --release --example large_input_benchmark
```

Environment for the latest captured run:

- Reference crates: `serde-saphyr 0.0.27` with `deserialize` only,
  `yaml-rust2 0.11.0`, `saphyr 0.0.6`
- Small fixture set: 33 files / 39 YAML documents / 25,362 bytes
- Large fixture set: pinned downstream fixtures plus generated 1 MiB inputs
- Captured: 2026-06-06 with Cargo's `release` profile and `--locked`

The linked `serde-saphyr` repository was ahead of crates.io at the time of this
capture (`0.0.28` in Git, latest published `0.0.27`). The benchmark pins the
published crate so the checked-in `Cargo.lock` and package checks remain
registry-reproducible.

The `serde-saphyr` rows use benchmark options rather than the crate defaults:
`strict_booleans: true` plus relaxed event, alias, document, node, scalar, and
merge budgets so the generated corpora are comparable throughput inputs. Because
`serde-saphyr` does not expose a native YAML value tree, the matched generic
Serde lane deserializes both libraries into `serde_yaml::Value`. The preflight
normalizes two public-contract differences before asserting equality:
`serde-saphyr::from_multiple_with_options` skips empty/null-like documents, and
serde-saphyr treats YAML tags as transparent for this target while saneyaml
preserves them.

The README overview graphic is a static summary of selected benchmark and
feature rows. Its source notes and update checklist live at
[`docs/assets/saneyaml-overview.md`](assets/saneyaml-overview.md); update that
note with this file whenever the graphic changes.

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
`saneyaml::parse_documents` moved from 25.87 to 23.98 ns/byte while `saphyr` moved
from 24.86 to 24.89 ns/byte. Retained output estimates are unchanged because
the returned tree shape is unchanged; the removed work is transient
per-document merge scanning and its scratch stack.

The 2026-06-01 plain-scalar continuation slice delays `String` allocation until
a plain scalar is proven to span multiple lines. In the next same-session target
capture, `generated_multi_doc_stream_1mib` `saneyaml::parse_documents` moved from
23.98 to 21.87 ns/byte while `saphyr` measured 24.42 ns/byte. Retained output
estimates are unchanged because single-line scalar output is identical; the
removed work is transient short `String` allocation before the scalar falls back
to the inline parse path.

The 2026-06-01 retained-capacity slice trims completed document, sequence, and
mapping vectors before returning parsed trees. In the large-input capture below,
`saneyaml::parse_documents` retained bytes moved from 703,340 to 486,188 on the
Stackable peak, from 23,031,972 to 13,006,500 on the generated multi-document
stream, and from 13,040,211 to 9,893,619 on the generated 1 MiB wide mapping.
The same-run speed lead over `saphyr` remains intact for the default spanful
parser on every large-input row.

The same milestone adds `saneyaml::parse_borrowed_documents`, an explicit
spanless retained tree that can borrow scalar strings from the caller's input
buffer. This is an additive load path, not a silent change to
`saneyaml::parse_documents`; the retained-output estimate counts the borrowed tree
heap and, like the `saphyr` row, does not count the caller-owned source buffer.
That row closes the retained-memory axis against `saphyr 0.0.6` across the
large-input corpus while preserving the owning parser's spans and scalar-source
behavior.

## Real-World Config Corpus

Latest same-run capture after adding the matched `serde-saphyr` lane, using
`YAML_BENCH_ITERS=1000`:

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 1,000 | 25,362 | 39 | 431.314 | 17.01 |
| `saneyaml::from_documents_str::<Value>` | 1,000 | 25,362 | 39 | 572.477 | 22.57 |
| `saneyaml::from_documents_str::<serde_yaml::Value>` | 1,000 | 25,362 | 39 | 578.399 | 22.81 |
| `saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>` | 1,000 | 25,362 | 39 | 1,165.027 | 45.94 |
| `serde_saphyr::from_multiple_with_options::<serde_yaml::Value>` | 1,000 | 25,362 | 39 | 1,055.332 | 41.61 |
| `serde_yaml::Value` stream | 1,000 | 25,362 | 39 | 644.638 | 25.42 |
| `yaml_rust2::YamlLoader` | 1,000 | 25,362 | 39 | 539.894 | 21.29 |
| `saphyr::Yaml::load_from_str` | 1,000 | 25,362 | 39 | 502.288 | 19.80 |

On this corpus, the matched generic Serde value lane measured saneyaml at
22.81 ns/byte versus serde-saphyr at 41.61 ns/byte. The private event-backed
prototype measured 45.94 ns/byte on the same target, so it is not a replacement
for the tree-backed Serde path yet. The raw tree-load rows are shown for context
but are a different contract from serde-saphyr's Serde-only API.

Same-turn pre-optimization baseline, captured before this milestone with the
default 200 iterations:

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 200 | 19,727 | 33 | 126.377 | 32.03 |
| `saneyaml::from_documents_str::<Value>` | 200 | 19,727 | 33 | 141.211 | 35.79 |
| `serde_yaml::Value` stream | 200 | 19,727 | 33 | 153.465 | 38.90 |
| `yaml_rust2::YamlLoader` | 200 | 19,727 | 33 | 100.959 | 25.59 |
| `saphyr::Yaml::load_from_str` | 200 | 19,727 | 33 | 92.393 | 23.42 |

Post zero-copy line-slice re-capture with 1,000 iterations (independent run,
2026-06-01):

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 1,000 | 19,727 | 33 | 355.271 | 18.01 |
| `saneyaml::from_documents_str::<Value>` | 1,000 | 19,727 | 33 | 422.618 | 21.42 |
| `serde_yaml::Value` stream | 1,000 | 19,727 | 33 | 523.390 | 26.53 |
| `yaml_rust2::YamlLoader` | 1,000 | 19,727 | 33 | 434.222 | 22.01 |
| `saphyr::Yaml::load_from_str` | 1,000 | 19,727 | 33 | 402.909 | 20.42 |

Result: after the zero-copy line slice, `saneyaml::parse_documents` was faster than
the pinned reference loaders on this small corpus in that 2026-06-01
1,000-iteration same-run capture (18.01 ns/byte vs `saphyr` at 20.42 and `yaml_rust2` at
22.01). The owning `Value` path also remains ahead of the `serde_yaml` `Value`
stream and roughly ties `yaml_rust2` on this corpus.

Post no-merge and plain-scalar fast-path re-capture with the default 200 iterations
(independent run, 2026-06-01):

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 200 | 19,727 | 33 | 72.598 | 18.40 |
| `saneyaml::from_documents_str::<Value>` | 200 | 19,727 | 33 | 84.585 | 21.44 |
| `serde_yaml::Value` stream | 200 | 19,727 | 33 | 116.031 | 29.41 |
| `yaml_rust2::YamlLoader` | 200 | 19,727 | 33 | 82.792 | 20.98 |
| `saphyr::Yaml::load_from_str` | 200 | 19,727 | 33 | 78.606 | 19.92 |

Methodology caveat: the pre-optimization table above was captured at 200
iterations and the post-optimization table at 1,000, so part of the across-table
ns/byte drop reflects warm-up rather than optimization. The trustworthy signal
is the same-run cross-loader comparison within each table, plus the larger,
lower-noise inputs below — not the across-table delta.

## Large Inputs

Command:

```sh
cargo run --locked --release --example large_input_benchmark
```

Default iterations: 20, controlled by `YAML_LARGE_BENCH_ITERS`.

### external_downstream_all

20 pinned downstream files / 245,062 bytes / 20 YAML documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 245,062 | 20 | 31.887 | 6.51 | 486,188 | 3,983 |
| `saneyaml::parse_borrowed_documents` | 20 | 245,062 | 20 | 30.256 | 6.17 | 173,556 | 904 |
| `saneyaml::from_documents_str::<Value>` | 20 | 245,062 | 20 | 40.035 | 8.17 | 217,483 | 3,780 |
| `saneyaml::from_documents_str::<serde_yaml::Value>` | 20 | 245,062 | 20 | 41.333 | 8.43 | 378,843 | 3,780 |
| `saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>` | 20 | 245,062 | 20 | 77.032 | 15.72 | 396,987 | 3,780 |
| `serde_saphyr::from_multiple_with_options::<serde_yaml::Value>` | 20 | 245,062 | 20 | 67.763 | 13.83 | 396,987 | 3,780 |
| `serde_yaml::Value` stream | 20 | 245,062 | 20 | 51.019 | 10.41 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 245,062 | 20 | 38.470 | 7.85 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 245,062 | 20 | 36.419 | 7.43 | 534,786 | 3,780 |

### stackable_dummy_cluster

One pinned Stackable CRD / 177,556 bytes / 1 YAML document.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 177,556 | 1 | 20.570 | 5.79 | 486,188 | 3,983 |
| `saneyaml::parse_borrowed_documents` | 20 | 177,556 | 1 | 19.159 | 5.40 | 173,556 | 904 |
| `saneyaml::from_documents_str::<Value>` | 20 | 177,556 | 1 | 24.524 | 6.91 | 217,483 | 3,780 |
| `saneyaml::from_documents_str::<serde_yaml::Value>` | 20 | 177,556 | 1 | 25.203 | 7.10 | 378,843 | 3,780 |
| `saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>` | 20 | 177,556 | 1 | 47.657 | 13.42 | 396,987 | 3,780 |
| `serde_saphyr::from_multiple_with_options::<serde_yaml::Value>` | 20 | 177,556 | 1 | 41.445 | 11.67 | 396,987 | 3,780 |
| `serde_yaml::Value` stream | 20 | 177,556 | 1 | 32.343 | 9.11 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 177,556 | 1 | 24.245 | 6.83 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 177,556 | 1 | 23.095 | 6.50 | 534,786 | 3,780 |

### generated_multi_doc_stream_1mib

Generated multi-document service stream / 1,048,680 bytes / 8,020 YAML
documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 1,048,680 | 8,020 | 367.321 | 17.51 | 13,006,500 | 128,321 |
| `saneyaml::parse_borrowed_documents` | 20 | 1,048,680 | 8,020 | 361.229 | 17.22 | 4,106,240 | 32,081 |
| `saneyaml::from_documents_str::<Value>` | 20 | 1,048,680 | 8,020 | 495.592 | 23.63 | 4,729,860 | 112,281 |
| `saneyaml::from_documents_str::<serde_yaml::Value>` | 20 | 1,048,680 | 8,020 | 538.155 | 25.66 | 9,862,660 | 112,281 |
| `saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>` | 20 | 1,048,680 | 8,020 | 1,116.471 | 53.23 | 11,607,364 | 112,281 |
| `serde_saphyr::from_multiple_with_options::<serde_yaml::Value>` | 20 | 1,048,680 | 8,020 | 1,223.597 | 58.34 | 11,607,364 | 112,281 |
| `serde_yaml::Value` stream | 20 | 1,048,680 | 8,020 | 661.464 | 31.54 | 11,607,364 | 112,281 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,680 | 8,020 | 545.253 | 26.00 | 10,386,948 | 112,281 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,680 | 8,020 | 534.700 | 25.49 | 14,770,560 | 112,281 |

### generated_wide_mapping_256kib

Generated one-document wide service mapping / 262,176 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 262,176 | 1 | 78.226 | 14.92 | 2,484,775 | 23,932 |
| `saneyaml::parse_borrowed_documents` | 20 | 262,176 | 1 | 73.898 | 14.09 | 765,792 | 2,994 |
| `saneyaml::from_documents_str::<Value>` | 20 | 262,176 | 1 | 107.359 | 20.47 | 938,236 | 17,950 |
| `saneyaml::from_documents_str::<serde_yaml::Value>` | 20 | 262,176 | 1 | 106.085 | 20.23 | 1,895,476 | 17,950 |
| `saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>` | 20 | 262,176 | 1 | 223.815 | 42.68 | 1,895,692 | 17,950 |
| `serde_saphyr::from_multiple_with_options::<serde_yaml::Value>` | 20 | 262,176 | 1 | 212.831 | 40.59 | 1,895,692 | 17,950 |
| `serde_yaml::Value` stream | 20 | 262,176 | 1 | 114.758 | 21.89 | 1,895,692 | 17,950 |
| `yaml_rust2::YamlLoader` | 20 | 262,176 | 1 | 102.762 | 19.60 | 1,704,220 | 17,950 |
| `saphyr::Yaml::load_from_str` | 20 | 262,176 | 1 | 97.245 | 18.55 | 2,393,312 | 17,950 |

### generated_wide_mapping_1mib

Generated one-document wide service mapping / 1,048,661 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 1,048,661 | 1 | 309.172 | 14.74 | 9,893,619 | 95,236 |
| `saneyaml::parse_borrowed_documents` | 20 | 1,048,661 | 1 | 282.013 | 13.45 | 3,047,520 | 11,907 |
| `saneyaml::from_documents_str::<Value>` | 20 | 1,048,661 | 1 | 419.435 | 20.00 | 3,739,059 | 71,428 |
| `saneyaml::from_documents_str::<serde_yaml::Value>` | 20 | 1,048,661 | 1 | 417.003 | 19.88 | 7,548,459 | 71,428 |
| `saneyaml::__unstable_event_serde::from_documents_str::<serde_yaml::Value>` | 20 | 1,048,661 | 1 | 878.512 | 41.89 | 7,548,675 | 71,428 |
| `serde_saphyr::from_multiple_with_options::<serde_yaml::Value>` | 20 | 1,048,661 | 1 | 843.638 | 40.22 | 7,548,675 | 71,428 |
| `serde_yaml::Value` stream | 20 | 1,048,661 | 1 | 477.817 | 22.78 | 7,548,675 | 71,428 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,661 | 1 | 420.142 | 20.03 | 6,786,771 | 71,428 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,661 | 1 | 401.299 | 19.13 | 9,523,712 | 71,428 |

Large-input story: after zero-copy line storage, the no-merge fast path,
delayed plain-scalar continuation allocation, and retained vector
right-sizing, `saneyaml::parse_documents` beats `yaml_rust2` and `saphyr` on every
large parser path in the latest capture on an unloaded machine. In the matched
Serde value lane, `saneyaml::from_documents_str::<serde_yaml::Value>` is faster
than `serde_saphyr::from_multiple_with_options::<serde_yaml::Value>` on every
large-input row. The hidden event-backed Serde prototype only wins against
serde-saphyr on the generated multi-document stream in this capture and remains
slower on the other large rows; it also retains the same `serde_yaml::Value`
output shape, so it does not improve retained output memory yet. The smallest
corpus (`external_downstream_all`) is the most contention-sensitive, so its
ordering is the first to wobble under load; the larger corpora hold a clearer
margin. The retained-memory story is now split
by output contract: the default spanful tree keeps spans and scalar-source
spellings and is faster than `saphyr`, while the additive
`saneyaml::parse_borrowed_documents` tree drops spans/source spellings and borrows
sliceable scalars, retaining less heap than `saphyr` on every large-input row
(for example, 3,047,520 vs 9,523,712 bytes on the 1 MiB wide mapping).

## Streaming And Compact Line-Table Milestone

This milestone adds a compact per-line table, a fused line-preprocessing scan,
source-backed borrowed scalars, and a lazy streaming line buffer that reclaims
consumed lines as `DocumentStream`/`EventStream` advance. Batch loaders keep an
eager line table for speed; only the streaming entrypoints reclaim. The input
string itself stays fully resident, so these are bounded-retention streaming
paths, not constant-memory readers.

Captured in a single `release` session against the in-repo harnesses:

```sh
YAML_BENCH_ITERS=1000 cargo run --locked --release --example real_world_benchmark
cargo run --locked --release --example dhat_memory -- --all
cargo run --locked --release --example conformance_compare
```

### Real-world config corpus (1,000 iterations)

33 files / 39 YAML documents / 25,362 bytes. This is a distinct, later capture
from a separate same-session run of this milestone, not the same measurement as
the "Real-World Config Corpus" table above; the corpus is identical but the
per-loader ns/byte figures differ run to run (for example `saphyr` reads 21.42
here versus 19.80 there), which is the run-to-run noise the methodology caveat
describes.

| parser/load path | ns/byte |
|---|---:|
| `saneyaml::parse_documents` | 15.03 |
| `saneyaml::from_documents_str::<Value>` | 21.19 |
| `saphyr::Yaml::load_from_str` | 21.42 |
| `yaml_rust2::YamlLoader` | 23.11 |
| `serde_yaml::Value` stream | 24.98 |

On this corpus `saneyaml::parse_documents` is the fastest load path; the owning
`Value` path ties `saphyr` and stays ahead of `yaml_rust2` and `serde_yaml`.

### Allocator-backed memory (dhat), 1 MiB multi-document stream

8,020 documents. `retained blocks` is the count of live allocations still held
while the parsed output is retained.

| path | allocs | bytes allocated | peak | retained blocks |
|---|---:|---:|---:|---:|
| `saneyaml` stream docs | 184,466 | 13.64 MB | 2.10 MB | 4 |
| `saneyaml` stream events | 232,594 | 49.28 MB | 2.11 MB | 6 |
| `saneyaml` borrowed | 80,219 | 17.29 MB | 6.21 MB | 32,081 |
| `saneyaml` owned | 200,519 | 16.05 MB | 15.12 MB | 128,321 |
| `saneyaml` Value | 449,140 | 25.07 MB | 15.12 MB | 112,281 |
| `yaml-rust2` | 585,478 | 29.29 MB | 17.15 MB | 192,481 |
| `saneyaml` as `serde_yaml::Value` | 465,180 | 39.73 MB | 20.79 MB | 136,341 |
| `saneyaml` event-backed as `serde_yaml::Value` | 1,114,806 | 175.38 MB | 22.83 MB | 136,341 |
| `serde-saphyr` as `serde_yaml::Value` | 577,465 | 59.71 MB | 21.79 MB | 136,344 |
| `serde_yaml` | 721,821 | 84.73 MB | 21.84 MB | 136,341 |
| `saphyr` | 216,559 | 22.77 MB | 22.30 MB | 192,481 |

On a multi-document stream the streaming loaders hold a bounded working set
(retained blocks stay at 4–6 regardless of stream length) and post the lowest
peak; the borrowed batch tree has the lowest peak among the non-streaming
loaders. The event-backed Serde prototype is allocation-heavy here because it
still consumes parser-recorded event frames rather than a direct parser-to-Serde
stream.

### Allocator-backed memory (dhat), 1 MiB wide single document

| path | peak | retained blocks |
|---|---:|---:|
| `serde-saphyr` as `serde_yaml::Value` | 10.73 MB | 83,337 |
| `yaml-rust2` | 10.98 MB | 130,951 |
| `saphyr` | 14.10 MB | 130,951 |
| `saneyaml` borrowed | 15.32 MB | 11,907 |
| `saneyaml` owned | 16.16 MB | 95,236 |
| `saneyaml` stream docs | 16.16 MB | 4 |
| `saneyaml` Value | 16.39 MB | 71,428 |
| `saneyaml` as `serde_yaml::Value` | 19.91 MB | 83,334 |
| `serde_yaml` | 23.42 MB | 83,334 |
| `saneyaml` stream events | 62.22 MB | 6 |
| `saneyaml` event-backed as `serde_yaml::Value` | 78.54 MB | 83,334 |

Streaming only helps when there are document boundaries to reclaim at. On a
single wide document there is nothing to reclaim mid-parse, so `yaml-rust2` and
`saphyr` post lower peaks than saneyaml on this shape, and the event-streaming
path is expensive here because it buffers per-event output for one large
document. The matched `serde-saphyr` value row posts a low wide-document peak,
while the event-backed Serde prototype is the highest peak in this capture.
Streaming is a multi-document memory win, not a universal one.

### Conformance (402 curated cases)

| library | spec accept/reject (400) | tree policy (2) |
|---|---:|---:|
| `saneyaml` | 400/400 | 2/2 |
| `yaml-rust2` | 400/400 | 2/2 |
| `saphyr` | 400/400 | 0/2 |
| `serde_yaml` | 333/400 | 2/2 |

saneyaml ties `yaml-rust2` and `saphyr` at 400/400 on the neutral spec set; it
is not a sole leader there. Its differentiation is the combination of full spec
conformance with tree-policy rejection of the duplicate-key/tree-error cases
that `saphyr` accepts, while `serde_yaml` trails the spec set at 333/400.

## Reproduction & Tooling

Every number in this document comes from an in-repo example, run under Cargo's
`release` profile. The commands below regenerate each captured table from a
source checkout of this repository; absolute values vary by machine, but the
same-run cross-loader ordering is the trustworthy signal on an otherwise-idle
machine. The harness is a hand-rolled `Instant::now()` loop with no warm-up or
statistics, so under heavy machine load even that ordering can invert; treat any
single capture as indicative rather than authoritative.

| captured section | checkout-only command |
|---|---|
| Real-World Config Corpus | `cargo run --locked --release --example real_world_benchmark` |
| Real-world corpus (1,000 iterations) | `YAML_BENCH_ITERS=1000 cargo run --locked --release --example real_world_benchmark` |
| Large Inputs (all corpora) | `cargo run --locked --release --example large_input_benchmark` |
| Large Inputs (custom iteration count) | `YAML_LARGE_BENCH_ITERS=20 cargo run --locked --release --example large_input_benchmark` |
| Allocator-backed memory (dhat) | `cargo run --locked --release --example dhat_memory -- --all` |
| dhat single (library, corpus) pair | `cargo run --locked --release --example dhat_memory -- saneyaml-borrowed multidoc` |
| Conformance (402 curated cases) | `cargo run --locked --release --example conformance_compare` |

Iteration counts default to 200 for `real_world_benchmark` (`YAML_BENCH_ITERS`)
and 20 for `large_input_benchmark` (`YAML_LARGE_BENCH_ITERS`). The
`dhat_memory` example installs a global allocator and must measure one library
per process; `-- --all` sweeps every `(library, corpus)` pair for you, and
`-- <library> <corpus>` profiles a single pair.

### Reference-crate versions

The captured comparison numbers were produced against these pinned
dev-dependency versions (see `Cargo.toml`):

| crate | version |
|---|---|
| `serde-saphyr` | 0.0.27 (`default-features = false`, `deserialize`) |
| `serde_yaml` | 0.9.34 |
| `saphyr` | 0.0.6 |
| `saphyr-parser` | 0.0.6 |
| `yaml-rust2` | 0.11.0 |
| `dhat` | 0.3.3 |

To reproduce against the exact pinned set, build with the checked-in
`Cargo.lock` (the default for `cargo run`). Bumping any reference crate can
shift its numbers, so re-capture the whole comparison table when upgrading.
