# Event-Backed Serde Workpad

This work tracks a saneyaml-native implementation of the useful ideas exposed by
the serde-saphyr comparison without copying serde-saphyr's architecture or
semantics.

## Current Shape

- `from_str`, `from_slice`, and `from_documents_str` parse into spanful `Node`
  trees before handing values to Serde.
- `Deserializer::from_str` is document-iterating, but it still owns parsed
  `Node` documents internally.
- `DocumentStream` bounds retained parsed documents, but reader-backed Serde
  entrypoints still read all input bytes first.
- `EventStream` exposes useful parser events, spans, tags, anchors, and document
  boundaries, but it currently records events while the tree parser is already
  constructing document nodes.
- The private event-backed Serde prototype now collects one document's event
  frame at a time for typed document iteration, but it is not wired into public
  entrypoints. A `#[doc(hidden)]` `__unstable_event_serde` wrapper exists only
  so source-checkout benchmarks can measure this path.

## Direction

The target design is a private event-backed Serde engine that eventually reads
from parser events directly instead of requiring a public `Node` tree first.
That path should serve typed Serde reads and reader-backed document iteration.
Tree, `Value`, spanful, and lossless APIs should remain tree/lossless backed.

The first vertical slice is intentionally narrow:

- deserialize ordinary typed structs, sequences, maps, options, scalars, and
  `IgnoredAny` from saneyaml events;
- preserve scalar source behavior for borrowed strings when spans allow it;
- retain scalar and collection tags for generic `Value` reads while honoring
  explicit YAML core scalar tags (`!!str`, `!!int`, `!!float`, `!!bool`,
  `!!null`) for typed reads, including directive-driven YAML 1.1 spellings;
- reject duplicate scalar and complex mapping keys, including alias-expanded
  keys, before Serde map targets can overwrite, including mappings reached
  through aliases or skipped by `IgnoredAny`;
- replay ordinary acyclic scalar, sequence, and mapping aliases through recorded
  event subtrees with the existing alias expansion budget;
- expand default/strict merge keys, merge lists, explicit target overrides, and
  YAML 1.1-compatible repeated/literal merge recovery;
- iterate typed document streams from string/slice input without retaining one
  full-stream event vector, and from reader input after the existing bounded
  read-to-end step;
- reject unsupported event features instead of silently accepting weaker
  semantics.

The current prototype lives in `src/event_de.rs` as a crate-private compiled
module. It proves the Serde visitor shape, per-document event-frame iteration,
reader-backed owned iteration after bounded input buffering, document-indexed
errors, borrowed-string behavior, duplicate-key rejection, tagged scalar and
collection projection, merge-key expansion/recovery, and
scalar/sequence/mapping alias replay for values and duplicate-key identity
without changing public entrypoints. Generic tagged collections and merge maps
currently use a temporary prepared `Node` handoff after event preflight; this
preserves semantics but is not the final live-event performance shape.

Current benchmark evidence keeps the path private: on the 2026-06-06 capture in
`docs/BENCHMARKS.md`, the hidden event-backed `serde_yaml::Value` lane is slower
than tree-backed saneyaml on every measured corpus. It only beats serde-saphyr
on the generated 1 MiB multi-document stream, and dhat shows higher allocation
traffic because the prototype still consumes parser-recorded event frames.

## Required Semantic Gates

The event-backed path must not become public or replace existing entrypoints
until it preserves these saneyaml contracts:

- YAML 1.1 merge recovery edge cases continue to match current semantic loading;
- aliases replay acyclic values under the same expansion and depth budgets;
- recursive or expansive aliases still fail before resource exhaustion;
- collection tags and tagged enum projection retain current typed Serde behavior;
- YAML 1.2 remains the default schema, with existing opt-in YAML 1.1 modes;
- errors retain document indexes, paths, spans, and related diagnostics.

## Benchmark Gates

Compare the new path against the current tree-backed Serde path and the pinned
`serde-saphyr` benchmark lane using the existing real-world, large-input, and
dhat examples. The saneyaml-native event path must parse equivalent input under
equivalent safety work; document skipping, tag stripping, duplicate bypassing,
or merge omission cannot count as a win. External `serde-saphyr` comparison rows
may keep unavoidable public-contract normalization only when it is explicit and
preflight-checked by the harness.

## 2026-06-06 Checkpoint

- Implementation: `src/event_de.rs` contains the private event-backed Serde
  engine, per-document typed iterators, reader-backed owned iteration, alias
  replay budgeting, duplicate-key preflight, merge expansion/recovery, tagged
  scalar/collection handling, and document-indexed errors.
- Benchmark surface: `saneyaml::__unstable_event_serde` exposes hidden
  collection-returning wrappers for source-checkout benchmarks only; it is
  excluded from the public API snapshot.
- Benchmark result: the event-backed `serde_yaml::Value` lane is semantically
  equivalent to tree-backed saneyaml in the benchmark preflight, but slower on
  most measured corpora and allocation-heavy in dhat. It should stay private
  until parser events can feed Serde without first constructing and recording
  tree-backed event frames.
- Verification: the checkpoint was checked with `cargo fmt --check`,
  `cargo check --locked --all-targets`, `cargo check --locked --examples`,
  `cargo test --locked --lib event_de`, the same event test under
  `--no-default-features`, `cargo test --locked --test streaming_api`,
  `cargo test --locked --test schema_modes`,
  `cargo test --locked --test trust_metadata`,
  `cargo test --locked --test runtime_dependency_closure`,
  `cargo package --locked --allow-dirty`, `scripts/check-public-api.sh`,
  and the real-world, large-input, and dhat benchmark commands documented in
  `docs/BENCHMARKS.md`.
