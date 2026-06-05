# Real-World Config Benchmarks And Large Inputs

The benchmark examples parse checked-in or generated YAML without timing file
I/O. They report aggregate cost so small files do not dominate the signal.
These benchmark and conformance commands are source-checkout-only: the published
crate package ships this document, but it intentionally excludes the
dev-dependency examples and fixture corpora used to regenerate the tables.

```sh
cargo run --release --example real_world_benchmark
YAML_BENCH_ITERS=1000 cargo run --release --example real_world_benchmark
cargo run --release --example large_input_benchmark
YAML_LARGE_BENCH_ITERS=20 cargo run --release --example large_input_benchmark
```

Environment for the latest captured run:

- Reference crates: `yaml-rust2 0.11.0`, `saphyr 0.0.6`
- Small fixture set: 33 files / 39 YAML documents / 25,362 bytes
- Large fixture set: pinned downstream fixtures plus generated 1 MiB inputs
- Captured: 2026-06-01 with Cargo's `release` profile

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

Corpus re-capture after adding CloudFormation/SAM, Symfony, GitLab CI,
CircleCI, Azure Pipelines, and an additional reusable GitHub Actions workflow:

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte |
|---|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 200 | 25,362 | 39 | 94.316 | 18.59 |
| `saneyaml::from_documents_str::<Value>` | 200 | 25,362 | 39 | 127.839 | 25.20 |
| `serde_yaml::Value` stream | 200 | 25,362 | 39 | 135.863 | 26.78 |
| `yaml_rust2::YamlLoader` | 200 | 25,362 | 39 | 108.641 | 21.42 |
| `saphyr::Yaml::load_from_str` | 200 | 25,362 | 39 | 100.915 | 19.89 |

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

Result: after the zero-copy line slice, `saneyaml::parse_documents` is faster than
the pinned reference loaders on this small corpus in the latest 1,000-iteration
same-run capture (18.01 ns/byte vs `saphyr` at 20.42 and `yaml_rust2` at
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
cargo run --release --example large_input_benchmark
```

Default iterations: 20, controlled by `YAML_LARGE_BENCH_ITERS`.

### external_downstream_all

20 pinned downstream files / 245,062 bytes / 20 YAML documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 245,062 | 20 | 37.887 | 7.73 | 486,188 | 3,983 |
| `saneyaml::parse_borrowed_documents` | 20 | 245,062 | 20 | 39.063 | 7.97 | 173,556 | 904 |
| `saneyaml::from_documents_str::<Value>` | 20 | 245,062 | 20 | 42.226 | 8.62 | 217,483 | 3,780 |
| `serde_yaml::Value` stream | 20 | 245,062 | 20 | 54.939 | 11.21 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 245,062 | 20 | 40.489 | 8.26 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 245,062 | 20 | 38.919 | 7.94 | 534,786 | 3,780 |

### stackable_dummy_cluster

One pinned Stackable CRD / 177,556 bytes / 1 YAML document.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 177,556 | 1 | 23.209 | 6.54 | 486,188 | 3,983 |
| `saneyaml::parse_borrowed_documents` | 20 | 177,556 | 1 | 24.466 | 6.89 | 173,556 | 904 |
| `saneyaml::from_documents_str::<Value>` | 20 | 177,556 | 1 | 25.708 | 7.24 | 217,483 | 3,780 |
| `serde_yaml::Value` stream | 20 | 177,556 | 1 | 34.069 | 9.59 | 396,987 | 3,780 |
| `yaml_rust2::YamlLoader` | 20 | 177,556 | 1 | 25.730 | 7.25 | 382,497 | 3,796 |
| `saphyr::Yaml::load_from_str` | 20 | 177,556 | 1 | 24.722 | 6.96 | 534,786 | 3,780 |

### generated_multi_doc_stream_1mib

Generated multi-document service stream / 1,048,680 bytes / 8,020 YAML
documents.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 1,048,680 | 8,020 | 472.805 | 22.54 | 13,006,500 | 128,321 |
| `saneyaml::parse_borrowed_documents` | 20 | 1,048,680 | 8,020 | 500.419 | 23.86 | 4,106,240 | 32,081 |
| `saneyaml::from_documents_str::<Value>` | 20 | 1,048,680 | 8,020 | 542.758 | 25.88 | 4,729,860 | 112,281 |
| `serde_yaml::Value` stream | 20 | 1,048,680 | 8,020 | 681.650 | 32.50 | 11,607,364 | 112,281 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,680 | 8,020 | 538.442 | 25.67 | 10,386,948 | 112,281 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,680 | 8,020 | 518.104 | 24.70 | 14,770,560 | 112,281 |

### generated_wide_mapping_256kib

Generated one-document wide service mapping / 262,176 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 262,176 | 1 | 86.486 | 16.49 | 2,484,775 | 23,932 |
| `saneyaml::parse_borrowed_documents` | 20 | 262,176 | 1 | 89.698 | 17.11 | 765,792 | 2,994 |
| `saneyaml::from_documents_str::<Value>` | 20 | 262,176 | 1 | 98.223 | 18.73 | 938,236 | 17,950 |
| `serde_yaml::Value` stream | 20 | 262,176 | 1 | 126.465 | 24.12 | 1,895,692 | 17,950 |
| `yaml_rust2::YamlLoader` | 20 | 262,176 | 1 | 102.841 | 19.61 | 1,704,220 | 17,950 |
| `saphyr::Yaml::load_from_str` | 20 | 262,176 | 1 | 93.165 | 17.77 | 2,393,312 | 17,950 |

### generated_wide_mapping_1mib

Generated one-document wide service mapping / 1,048,661 bytes.

| parser/load path | iterations | bytes per iteration | docs per iteration | elapsed ms | ns/byte | peak retained bytes | peak retained heap objects |
|---|---:|---:|---:|---:|---:|---:|---:|
| `saneyaml::parse_documents` | 20 | 1,048,661 | 1 | 355.523 | 16.95 | 9,893,619 | 95,236 |
| `saneyaml::parse_borrowed_documents` | 20 | 1,048,661 | 1 | 374.905 | 17.88 | 3,047,520 | 11,907 |
| `saneyaml::from_documents_str::<Value>` | 20 | 1,048,661 | 1 | 417.223 | 19.89 | 3,739,059 | 71,428 |
| `serde_yaml::Value` stream | 20 | 1,048,661 | 1 | 503.740 | 24.02 | 7,548,675 | 71,428 |
| `yaml_rust2::YamlLoader` | 20 | 1,048,661 | 1 | 412.899 | 19.69 | 6,786,771 | 71,428 |
| `saphyr::Yaml::load_from_str` | 20 | 1,048,661 | 1 | 371.689 | 17.72 | 9,523,712 | 71,428 |

Large-input story: after zero-copy line storage, the no-merge fast path,
delayed plain-scalar continuation allocation, and retained vector
right-sizing, `saneyaml::parse_documents` beats `yaml_rust2` and `saphyr` on every
large parser path in the latest capture on an unloaded machine. The smallest
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
YAML_BENCH_ITERS=1000 cargo run --release --example real_world_benchmark
cargo run --release --example dhat_memory -- --all
cargo run --release --example conformance_compare
```

### Real-world config corpus (1,000 iterations)

33 files / 39 YAML documents / 25,362 bytes. This is a distinct, later capture
from a separate same-session run of this milestone, not the same measurement as
the "Real-World Config Corpus" table above; the corpus is identical but the
per-loader ns/byte figures differ run to run (for example `saphyr` reads 21.42
here versus 19.89 there), which is the run-to-run noise the methodology caveat
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

8,020 documents. `retained blocks` is the count of live allocations at peak.

| path | allocs | bytes allocated | peak | retained blocks |
|---|---:|---:|---:|---:|
| `saneyaml` stream docs | 184,466 | 13.64 MB | 2.10 MB | 4 |
| `saneyaml` stream events | 232,594 | 49.28 MB | 2.11 MB | 6 |
| `saneyaml` borrowed | 80,219 | 17.29 MB | 6.21 MB | 32,081 |
| `saneyaml` owned | 200,519 | 16.05 MB | 15.12 MB | 128,321 |
| `yaml-rust2` | 585,478 | 29.29 MB | 17.15 MB | 192,481 |
| `serde_yaml` | 721,821 | 84.73 MB | 21.84 MB | 136,341 |
| `saphyr` | 216,559 | 22.77 MB | 22.30 MB | 192,481 |

On a multi-document stream the streaming loaders hold a bounded working set
(retained blocks stay at 4–6 regardless of stream length) and post the lowest
peak; the borrowed batch tree has the lowest peak among the non-streaming
loaders.

### Allocator-backed memory (dhat), 1 MiB wide single document

| path | peak | retained blocks |
|---|---:|---:|
| `yaml-rust2` | 10.98 MB | 130,951 |
| `saphyr` | 14.10 MB | 130,951 |
| `saneyaml` borrowed | 15.32 MB | 11,907 |
| `saneyaml` owned | 15.66 MB | 95,236 |
| `saneyaml` stream docs | 16.16 MB | 4 |
| `saneyaml` stream events | 62.22 MB | 6 |

Streaming only helps when there are document boundaries to reclaim at. On a
single wide document there is nothing to reclaim mid-parse, so `yaml-rust2` and
`saphyr` post lower peaks than saneyaml on this shape, and the event-streaming
path is the worst here because it buffers per-event output for one large
document. Streaming is a multi-document memory win, not a universal one.

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
| Real-World Config Corpus | `cargo run --release --example real_world_benchmark` |
| Real-world corpus (1,000 iterations) | `YAML_BENCH_ITERS=1000 cargo run --release --example real_world_benchmark` |
| Large Inputs (all corpora) | `cargo run --release --example large_input_benchmark` |
| Large Inputs (custom iteration count) | `YAML_LARGE_BENCH_ITERS=20 cargo run --release --example large_input_benchmark` |
| Allocator-backed memory (dhat) | `cargo run --release --example dhat_memory -- --all` |
| dhat single (library, corpus) pair | `cargo run --release --example dhat_memory -- saneyaml-borrowed multidoc` |
| Conformance (402 curated cases) | `cargo run --release --example conformance_compare` |

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
| `serde_yaml` | 0.9.34 |
| `saphyr` | 0.0.6 |
| `saphyr-parser` | 0.0.6 |
| `yaml-rust2` | 0.11.0 |
| `dhat` | 0.3.3 |

To reproduce against the exact pinned set, build with the checked-in
`Cargo.lock` (the default for `cargo run`). Bumping any reference crate can
shift its numbers, so re-capture the whole comparison table when upgrading.
