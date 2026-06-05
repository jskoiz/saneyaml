# README Overview Graphic Source Note

`saneyaml-overview.png` is a static README summary. This note is the source
ledger for the claims embedded in that image and should be reviewed whenever the
image, README feature copy, or benchmark tables change.

## Benchmark Slice

The speed chart in the graphic comes from
[`docs/BENCHMARKS.md`](../BENCHMARKS.md), under "Streaming And Compact
Line-Table Milestone" -> "Real-world config corpus (1,000 iterations)".

Source-checkout-only captured command:

```sh
YAML_BENCH_ITERS=1000 cargo run --release --example real_world_benchmark
```

Graphic data:

| parser/load path | ns/byte |
|---|---:|
| `saneyaml::parse_documents` | 15.03 |
| `saphyr::Yaml::load_from_str` | 21.42 |
| `yaml_rust2::YamlLoader` | 23.11 |
| `serde_yaml::Value` stream | 24.98 |

These are single best-case captures from one idle-machine run; the cross-loader
margin is often tighter and can shift run to run, so treat them as indicative.

Important caveats:

- Lower ns/byte is better.
- The chart is a same-run, 1,000-iteration capture of the checked-in real-world
  config corpus: 33 files, 39 YAML documents, and 25,362 bytes.
- The benchmark excludes file I/O. It measures parser/load paths over already
  available YAML text.
- Treat the graphic as a summary of this captured corpus, not a universal speed
  claim for every YAML shape.
- The graphic does not make a peak-memory claim. Keep memory wording tied to
  the exact retained-output, allocation, or allocator-backed rows in
  `docs/BENCHMARKS.md`. In particular, the single wide-document allocator table
  shows lower peaks for `yaml-rust2` and `saphyr` than several saneyaml paths.

## Feature Matrix Scope

The feature matrix is a compact public overview, not a compatibility spec. The
source-of-truth references are:

| graphic row | source to review |
|---|---|
| Serde-first API | `README.md`, `docs/MIGRATION.md`, `docs/COMPATIBILITY.md` |
| YAML 1.2 (no "Norway") | `README.md`, `docs/COMPATIBILITY.md` scalar resolution notes |
| `forbid(unsafe_code)` | `src/lib.rs` crate attributes |
| Built-in resource limits | `README.md`, `SECURITY.md` resource-limit posture |
| Lossless comment-preserving edit | `README.md`, `docs/ARCHITECTURE.md`, `docs/COMPATIBILITY.md` |
| Maintained | current repository status plus upstream repository status at review time |

Before a release that changes the image, re-check upstream package status for
`serde_yaml`, `yaml-rust2`, and `saphyr`; maintenance status can drift without
any code change in this repository.

## Update Checklist

When regenerating or editing `saneyaml-overview.png`:

1. Update `docs/BENCHMARKS.md` first with the measured command, corpus size,
   pinned reference crate versions, and caveats.
2. Keep benchmark comparisons within a same-run table unless the text explicitly
   explains why cross-run or cross-corpus comparisons are valid.
3. Keep memory claims out of the graphic unless the exact allocator or retained
   metric is named and sourced.
4. Update this source note with the exact chart values and feature-source rows.
5. Re-read the README image alt text and caption so they point to the current
   source note and do not broaden the claim.
