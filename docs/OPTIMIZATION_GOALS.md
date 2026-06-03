# saneyaml optimization goals (deep work)

Five high-effort optimization tasks aimed at turning the current benchmark
story ("competitive, ~4× less retained memory than saphyr") into a decisive one
("lowest peak, fewest allocations, fastest"). Each goal is grounded in measured
data from the in-repo harnesses, not speculation.

## Baseline (measured, 1 MiB generated corpora)

Reproduce with:

```
cargo run --release --example dhat_memory -- --all          # allocator-backed memory
cargo run --release --example conformance_compare           # spec conformance
cargo run --release --example real_world_benchmark          # throughput
```

1 MiB multi-document stream, single parse, after the line-table pre-size fix:

| Library | ns/byte | Total allocs | Bytes allocated | Retained | Peak |
|---|---:|---:|---:|---:|---:|
| saneyaml (borrowed) | ~18.5 | 425,098 | 54.7 MB | 4.1 MB | 22.6 MB |
| saphyr | ~19.6 | 216,559 | 22.8 MB | 21.3 MB | 22.3 MB |
| yaml-rust2 | ~21.3 | 585,478 | 29.3 MB | 16.1 MB | 17.1 MB |
| serde_yaml | ~26.0 | 721,821 | 84.7 MB | 20.7 MB | 21.8 MB |

**Invariant for every goal below:** all existing tests stay green
(`cargo test`), conformance stays 400/400 on the spec axis and 2/2 on the
tree-policy axis (`examples/conformance_compare`), and `#![forbid(unsafe_code)]`
is preserved. Every PR must re-run `dhat_memory --all` and paste before/after.

---

## Goal 1 — Shrink the line table to cut peak memory below yaml-rust2

**Why.** dhat attributes ~13.6 MB of the 22.6 MB peak to a single allocation:
the `Vec<Line>` built in `preprocess()` (`src/parse.rs:3024`). Each `Line` is
~112 bytes because it embeds **two `LineText` structs, each carrying an
`Rc<str>` (16 B) plus two `usize` offsets**, and the corpus is ~120k short
lines. The `Rc` does not duplicate text but bloats the struct array.

**Target.** `Line` ≤ 40 bytes; line table ~4 MB; **peak ≤ 14 MB** (below
yaml-rust2's 17.1 MB), making saneyaml best-in-class on peak.

**Approach.**
- Store the source `Rc<str>` (or `&str`) **once** on the `Parser`, not per line.
- Replace each `LineText` with offset pairs; change `start/end/indent/
  content_start/no` from `usize` to `u32` (inputs are size-limited well under
  4 GiB — assert this in `check_input_len`).
- Drop the redundant `content: LineText`; derive content from `raw` +
  `content_start` + a stored `content_end: u32`.
- Rework `LineText`'s `Deref<str>` and the accessors (`raw_from`,
  `raw_content_from`, `span`, `local_span`) to resolve against the parser-held
  source. This is the invasive part — it touches every line accessor across the
  5,500-line `parse.rs`.

**Acceptance.** `std::mem::size_of::<Line>()` assertion in a test; peak from
`dhat_memory` ≤ 14 MB on both corpora; full suite green.

**Risk.** High surface area in the hottest file. Land behind thorough tests;
consider an intermediate commit that only removes the duplicate `content`
field before the larger offset rework.

---

## Goal 2 — Halve transient allocation churn in the node-building hot path

**Why.** saneyaml makes **425k allocations / 54.7 MB churn** to saphyr's
**216k / 22.8 MB**. The dhat call-site profile shows the churn concentrated in
`Parser::parse_mapping`, `parse_sequence`, `parse_scalar_with_schema`,
`emit_scalar_node`, and `key_identity::check_duplicate_at_depth_limit` (the
last allocates ~8.5 MB total, freed immediately — pure transient cost). Most
are per-node temporaries: small `Vec`/`HashMap`/`String` allocated and dropped
per collection or per scalar.

**Target.** Total allocations ≤ 250k and bytes allocated ≤ 30 MB on the
multidoc corpus (parity with saphyr), with no change to retained memory.

**Approach.**
- Profile each hot site (`dhat_memory --profile saneyaml-borrowed multidoc`,
  view `dhat-heap.json`).
- Replace per-collection `Vec`/`HashMap` temporaries with reusable scratch
  buffers owned by the `Parser` and cleared between uses.
- Use `SmallVec`/inline storage for the common small-collection case (most
  config mappings/sequences are tiny).
- Rework duplicate-key detection so it does not allocate per mapping entry
  (see Goal 4 for the data-structure angle).

**Acceptance.** `dhat_memory --all` shows the allocation/byte targets; no
regression in `real_world_benchmark`; suite green.

---

## Goal 3 — Lazy per-document line preprocessing (O(largest doc), not O(stream))

**Why.** `preprocess()` materializes the **entire** stream's line table before
parsing begins, so memory and first-document latency scale with total input,
not document size. For Helm/Kubernetes-style multi-doc streams this is the
wrong cost model and caps the streaming story.

**Target.** Peak memory for an N-document stream bounded by the largest single
document, not the sum. Add a `docs/sec` + `first-document-latency` metric to
`large_input_benchmark` and show it flat as stream length grows.

**Approach.**
- Convert line preprocessing to a lazy iterator that yields lines on demand,
  advancing document-by-document, so the line table for already-emitted
  documents can be released.
- Ensure the borrowed-tree contract still holds (scalars borrow from the
  retained input slice, which stays alive; only per-doc line metadata is freed).
- Wire `DocumentStream`/`EventStream` to pull lazily end-to-end.

**Acceptance.** A test parsing a 16 MiB / many-doc stream shows peak within ~2×
of a single large document; streaming entrypoints never build the whole table.

**Risk.** Architectural. Largest of the five; sequence it after Goals 1–2.

---

## Goal 4 — Drive borrowed-path scalar allocations toward zero (true zero-copy)

**Why.** On the borrowed path, retained blocks are 32k for ~8k documents —
roughly 4 live allocations per document. The profile shows
`parse_scalar_with_schema` and `ast::Node::with_scalar_source` allocating
per scalar (24k–48k blocks). A borrowed tree should allocate for a scalar
**only** when escapes or multiline folding force an owned `String`; plain
scalars (the overwhelming majority in configs) should be pure `&str`/`Cow::
Borrowed` slices into the input.

**Target.** Retained blocks ≤ 10k on the multidoc corpus; a test proving plain
scalars produce zero per-scalar heap allocations.

**Approach.**
- Trace scalar construction from `parse_scalar_with_schema` →
  `emit_scalar_node` → `Node::with_scalar_source` and confirm the `Cow` stays
  `Borrowed` unless transformation is required.
- Eliminate intermediate owned buffers for plain/unescaped scalars.
- Add a dhat-backed test asserting allocation count for an all-plain-scalar
  fixture is below a tight threshold.

**Acceptance.** `dhat_memory` retained-blocks target met; widen the retained-
memory lead over saphyr beyond the current ~4×.

---

## Goal 5 — Single-pass preprocessing + SIMD newline/indent scanning for throughput

**Why.** saneyaml is only ~6% faster than saphyr. `preprocess()` currently does
multiple passes per line: `split_inclusive('\n')`, then trim, then a separate
comment scan, then a byte-by-byte indent count. Newline counting and indent
scanning are byte searches that vectorize well.

**Target.** ≤ 15 ns/byte on `real_world_benchmark` (from ~18.5), clearly the
fastest of the four libraries; no behavior change.

**Approach.**
- Use `memchr`/`memchr_iter` for newline splitting and the indent/comment scan
  instead of char-by-char iteration.
- Fold the per-line trim + comment-detection + indent passes into a single scan.
- Pre-size the line table is already done (`preprocess`); extend with reusable
  buffers from Goal 2.
- Benchmark each change; keep only wins that survive 3+ runs.

**Acceptance.** `real_world_benchmark` ns/byte target met across 3 runs;
`dhat_memory` shows no allocation regression; suite green.

---

## Suggested sequence

1. **Goal 1** (peak win, self-contained, unlocks the "lowest peak" claim).
2. **Goal 2** (churn parity with saphyr; shares scratch-buffer plumbing).
3. **Goal 4** (zero-copy scalars; widens the retained-memory lead).
4. **Goal 5** (throughput; builds on Goal 2 buffers).
5. **Goal 3** (streaming architecture; largest, do last).
