# Release Notes

## 0.1.0 release-candidate work in progress

This crate is still a local developer preview and is not published. Current
release blockers are the public crate name, final license, version decision,
and explicit user approval for a crates.io push.

Notable completed release-candidate behavior:

- Default loaded-tree and Serde reads expand untagged merge keys while raw
  parser events retain merge syntax.
- Explicit YAML core scalar tags retain tag/source metadata and support typed
  Serde reads for strings, booleans, nulls, integers, floats, timestamps, and
  binary byte targets.
- YAML 1.1 collection tags `!!set`, `!!omap`, and `!!pairs` retain tagged
  collection payloads in `Node`/`Value` and support typed Serde reads for
  set-like targets, ordered pair sequences, `!!omap` map targets, and
  duplicate-preserving `!!pairs` pair sequences.
- The migration harness records YAML 1.1 scalar construction as an explicit
  call-site choice, including the default decimal treatment of `0123` versus
  YAML 1.1 octal interpretation under `LoadOptions`.
- YAML 1.1 conformance fixtures now cover directive-driven legacy scalar
  construction, null and float spellings, timestamp time-zone forms, explicit
  binary and collection tags, invalid binary typed-target diagnostics,
  merge-key expansion under legacy schema selection, and boolean/numeric-key
  duplicate diagnostics.
- `parse_lossless` / `LosslessStream` preserve comments, trivia, anchors,
  aliases, and stable graph ids for source-backed inspection, replay, and
  validated node/source-span edits, insertions, and deletions that preserve
  untouched bytes.
- Lossless source edits and graph identity now have fuzz-corpus replay covering
  scalar, flow mapping, flow sequence replacements, YAML 1.1 merge/comment
  streams, anchor redefinition, and recursive aliases.
- Direct Serde `IgnoredAny` entrypoints now validate malformed YAML and
  single-document boundaries before skipping a document.
- Divergence records now require caller-facing `migration_impact` text, so
  compatibility decisions are tied to adoption risk instead of only parser
  policy. The pinned Psych/libyaml probe records alias object identity as a
  deliberate semantic-tree divergence covered by `LosslessStream`.

Known release-candidate gaps remain tracked in `BASELINE.md`,
`COMPATIBILITY.md`, and `MIGRATION.md`: complete YAML 1.1 ecosystem parity,
full structural lossless formatting beyond validated source-span editing, semantic
`Node`/`Value` graph identity, final package metadata, and external
publication.
