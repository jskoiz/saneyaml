# Code Review Disposition for `7b57b37`

This document closes out the findings in `git show 7b57b37:docs/CODE_REVIEW.md`
across the merged PRs `#47` through `#55` and the
`c1/review-disposition-followup` branch. Each item is either fixed,
documented as an intentional contract, refuted with evidence, or left as a named
follow-up where the review item is broader than this fix branch.

## Top Findings

| # | Status | Disposition | Proof surface |
|---|--------|-------------|---------------|
| 1 | Fixed | Multi-line unterminated flow collection close detection is incremental instead of full-buffer rescanning. Accumulation now checks nesting depth while scanning newly appended text. | PR #55; `tests/dos_hardening.rs::multiline_unterminated_flow_is_rejected_quickly`; `tests/dos_hardening.rs::multiline_unterminated_flow_scales_subquadratically` |
| 2 | Fixed with documented divergence | YAML 1.1 sexagesimal parsing now uses base-60 positional weights, so `1:20` is `80` and `1:20.5` is `80.5`. Docs and divergence records now say this intentionally follows the spec-weighted/PyYAML-style interpretation even though the pinned Psych/libyaml probe keeps the old two-part behavior. | `tests/schema_modes.rs`, `tests/yaml11_conformance.rs`, `tests/serde_value_api.rs`, compatibility harness/divergence records |
| 3 | Fixed | Float emission uses the shortest round-tripping form via the existing `ryu` path and now agrees with `Number` display semantics. | PR #49; `cargo test --locked --test emitter` |
| 4 | Fixed by choosing one contract | Bytes serialization now rejects consistently across `to_value`, value serializers, string/writer serializers, and streaming serializers. Read-side `!!binary` support remains explicitly tag-driven for typed byte targets. | `tests/serde_value_api.rs`, `tests/serde_yaml_swap_harness.rs`, `tests/fixtures/divergences/records/byte-deserialization.toml` |
| 5 | Fixed for caller-built nodes | Caller-built owned and borrowed `Node` deserializers expand untagged and explicit merge-tag `<<` keys by default. Parser-produced `InputNode` keeps the parser-applied schema policy and is not re-expanded strictly, preserving YAML 1.1 invalid-payload recovery. | `tests/serde_value_api.rs::serde_api_caller_built_node_deserializers_expand_merge_keys_by_default`; merge docs/records |
| 6 | Fixed | Explicit `!!str` numeric scalars no longer use i128/u128-only coercion. Explicit strings remain strings for numeric targets; explicit `!!int` remains the numeric path. | `tests/serde_value_api.rs::serde_api_explicit_str_numeric_scalars_do_not_coerce_to_integers`; docs |
| 7 | Fixed with one retained policy | Adjacent UTF-16 surrogate-pair `\u` escapes are combined; lone or mismatched surrogates reject. The literal-tab escape nit remains accepted because the YAML test-suite selection currently expects that compatibility behavior. | `tests/yaml_test_suite.rs::yts_parse_double_quoted_utf16_surrogate_pair_escape`; targeted YAML-suite double-quoted tests |
| 8 | Fixed | `!!omap` deserialization into Rust map targets validates duplicate keys before handing entries to Serde, matching the crate's strict duplicate-key policy. Pair-sequence targets still preserve ordered duplicates. | `tests/yaml11_conformance.rs::yaml11_omap_map_targets_reject_duplicate_keys_before_deduping`; collection-tag divergence record |
| 9 | Fixed | `BYTES_UNSUPPORTED` now says bytes serialization is not implemented, matching the writer-side failure mode and preserving read-side `!!binary` wording. | `tests/serde_value_api.rs`; `tests/serde_yaml_swap_harness.rs` |
| 10 | Fixed | `ConfigEditor::apply` no longer performs the discarded extra reparse, and lossless override marking now uses a borrowed `HashSet` instead of an owned linear scan. | PR #51; full lossless tests through `cargo test --locked` |
| 11 | Fixed | `Number::partial_cmp` now returns `None` whenever either side is NaN, including int/float comparisons, and `ValueVisitor::visit_f64` normalizes NaN through `Number::from`. | `tests/serde_value_api.rs`; `src/ast.rs` |
| 12 | Partially fixed and documented | Alias-depth accounting and alias clone/count/depth work were made iterative for the regression path, and the off-by-one was fixed. The public opt-out still removes the general parser depth guard, so docs continue to warn that adversarially deep trusted-input opt-outs can exhaust runtime stack in other recursive paths. | PR #54 docs; `tests/dos_hardening.rs::nesting_depth_opt_out_alias_expansion_uses_iterative_accounting` |
| 13 | Fixed | `Span`/`Location` documentation now states columns are UTF-8 byte columns, and the redundant start-boundary clamp in span rendering was removed. | PR #54; doc tests/full tests |
| 14 | Fixed | `CHANGELOG.md` documents the shipped `EnumRepresentation` and `EmitOptions::with_enum_representation` API. | PR #47; `tests/trust_metadata.rs` |
| 15 | Fixed | Benchmark docs were reconciled: duplicate corpus tables were relabeled or corrected, methodology wording was softened, stale memory figures were refreshed, and the overview graphic now qualifies the headline as an idle-machine/best-case figure. | PR #53; `docs/BENCHMARKS.md`; `docs/assets/saneyaml-overview.md` |
| 16 | Fixed by rewording | `SECURITY.md` now describes fuzz sweeps as local release hygiene rather than committed evidence, while preserving the trust-test substring for the 1000-run operator expectation. | PR #48; `cargo test --locked --test trust_metadata` |

## Secondary Findings

| Finding | Status | Disposition |
|---------|--------|-------------|
| Leading-zero sexagesimal first groups and underscore-in-sexagesimal handling | Documented as follow-up | The main 60x correctness bug is fixed. The lexical edge policy is left for a focused YAML 1.1 scalar-grammar cleanup because changing it may move additional divergence records. |
| Timestamp fractions longer than 9 digits | Fixed | YAML 1.1 timestamp parsing truncates fractional seconds to nanoseconds instead of rejecting extra nonzero digits. |
| Alias-depth off-by-one | Fixed | Alias expansion depth now accounts for replacing an alias node with the target depth without adding an extra level. |
| `parse.rs` dead `reclaim_consumed` field | Fixed | The always-true guard and field were removed. |
| `parse.rs` unreachable `expect("matched digit")` | Fixed | Replaced with a non-panicking default in the guarded branch. |
| `FlowParser::skip_ws` redundant peek/expect | Fixed | The loop now peeks once and bumps the matched whitespace character without an `expect`. |
| `Tag::Display` duplicate arm | Fixed | The redundant arm was collapsed into the fallback arm. |
| `error.rs` redundant `floor_char_boundary` on `caret_start` | Fixed | The start clamp now relies on `line_bounds` having already proved the boundary. |
| `node_path_segment` cloning key subtrees | Fixed | Error path segment construction now borrows `NodeValue` directly instead of converting through a cloned `Value`. |
| `with.rs` forwarder boilerplate | Refuted | The review correctly identified this as ordinary Serde forwarder boilerplate; no change made. |
| Historical "ten fuzz targets" wording | Refuted | The line is a frozen 0.1.0 changelog entry and matched the v0.1.0 fuzz target count. |
| Ruby/Psych probe test skipping when Ruby is absent | Documented as accepted | This remains an optional local oracle probe rather than a portable CI gate. The primary compatibility gates are the committed Rust tests and fixture records. |
| Collection tag error-path key context | Follow-up | The duplicate-key correctness issue is fixed. Broader path-quality unification across the repeated `!!omap`/`!!pairs`/`!!set` accessors is a refactor follow-up, not a blocking correctness change for this branch. |

## Meta-Test And Coverage Claims

The review's "rigor theater" criticism is accepted as a documentation and future
test-design concern, not as evidence that the behavioral conformance gates are
fake. The branch keeps the existing manifest tests because they catch ledger and
documentation drift, but docs now describe the 402 cases as a curated selected
set rather than implying a complete upstream suite. The real behavioral surfaces
remain `yaml_test_suite`, `event_parity`, `tree_parity`, `dos_hardening`,
`serde_value_api`, `schema_modes`, and the compatibility/divergence harnesses.

Turning the manifest tests into parser-executing tests is a named follow-up. It
should be done as a test-harness redesign so the manifest layer imports the same
case lists used by the behavioral tests instead of scraping or duplicating
source literals.

## Verification Checklist

Targeted proof should include:

- `cargo test --locked --test serde_value_api -- --nocapture`
- `cargo test --locked --test yaml11_conformance -- --nocapture`
- `cargo test --locked --test yaml_test_suite double_quoted -- --nocapture`
- `cargo test --locked --test dos_hardening -- --nocapture`
- `cargo test --locked --test schema_modes -- --nocapture`
- `cargo test --locked --test serde_yaml_swap_harness -- --nocapture`
- `cargo test --locked --test compatibility_harness -- --nocapture`
- `cargo test --locked --test emitter -- --nocapture`
- `cargo test --locked --test trust_metadata -- --nocapture`
- `cargo test --locked --test divergence_manifest -- --nocapture`
- `cargo test --locked --test parity_manifest -- --nocapture`

Broad proof should include `cargo fmt --all --check`, `git diff --check`,
`cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`,
`RUSTDOCFLAGS='-D missing_docs' cargo doc --locked --no-deps`,
`scripts/check-public-api.sh`, `scripts/check-feature-clippy.sh`,
and `cargo package --locked --allow-dirty`. The requested
`cargo test --locked --test baseline_audit` gate is unavailable in this checkout:
Cargo reports no test target named `baseline_audit`.
