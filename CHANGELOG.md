# Changelog

This changelog follows the shape of Keep a Changelog.

Policy: user-visible changes land under `Unreleased` as they merge, grouped by
the standard `Added`/`Changed`/`Deprecated`/`Removed`/`Fixed`/`Security`
headings. On release, the `Unreleased` heading is renamed to the new version
number and a fresh, empty `Unreleased` section is added at the top. Entries
describe the human-visible change only; internal refactors with no observable
effect are omitted.

## Unreleased

## 0.3.0

### Added

- Added `EmitOptions::with_yaml_1_1_safe_strings`, an opt-in emitter setting that
  quotes plain string scalars which YAML 1.2 keeps as strings but YAML 1.1 /
  `serde_yaml`-style readers would resolve to booleans or numbers (`no`, `yes`,
  `on`, `off`, sexagesimals like `12:34:56`, and octal/hex/binary integers), so
  emitted strings round-trip through YAML 1.1 consumers as well. Disabled by
  default, preserving the minimally quoted YAML 1.2 structural output.
- Added `EmitOptions::with_enum_representation`, an opt-in emitter setting that
  selects how Serde enum variants are written: `EnumRepresentation::Tag` (the
  default) keeps the YAML tag shape, while `EnumRepresentation::SingletonMap`
  emits each variant as a single-key map globally, matching
  `serde_yaml::with::singleton_map` writer output without annotating every enum
  field.

### Changed

- YAML 1.1 two-part sexagesimal scalars now resolve with base-60 positional
  weighting (`1:20` is `80`, `1:20.5` is `80.5`), matching the YAML 1.1 spec and
  PyYAML. This intentionally diverges from the pinned Psych/libyaml hour:minute
  construction that produced `4800`; three-part sexagesimals are unchanged. Code
  that relied on the previous two-part values must adjust.
- Byte serialization is now rejected consistently across `to_value`, the value
  serializers, and the string/writer/streaming serializers, instead of `to_value`
  silently lowering bytes to a `u8` sequence. Read-side `!!binary` decoding into
  byte targets is unchanged.
- Explicit `!!str`-tagged numeric scalars (for example `!!str 7`) now stay strings
  for every integer and float Serde target instead of being coerced to integers
  only for `i128`/`u128`. `from_str` and `from_value` now agree on these inputs.
- Caller-built owned and borrowed `Node` / `Value` Serde deserializers now expand
  `<<` merge keys by default, matching `from_value`; parser-produced reads keep
  their schema-driven recovery behavior.
- Floats are now emitted in shortest round-tripping form (for example `1e308`
  rather than a 300-plus digit expansion), matching `Number`'s display output.
- `Span` / `Location` columns are documented as one-based UTF-8 byte columns.

### Fixed

- Combined UTF-16 surrogate-pair escapes (for example `"😀"`) in
  double-quoted scalars into the intended astral code point; lone or mismatched
  surrogate escapes are rejected.
- `!!omap` deserialized into a map target now rejects duplicate keys, matching the
  crate's strict duplicate-key policy, instead of silently keeping the last value.
  Pair-sequence targets still preserve ordered duplicates.
- `Number::partial_cmp` now returns `None` whenever either operand is `NaN`,
  including integer/float comparisons, and `NaN` is sign-normalized when
  deserialized into `Value`.
- Emitting a verbatim tag whose suffix contains `>` now percent-escapes it, so the
  emitted tag round-trips through the parser instead of producing invalid YAML.
- Merge-key expansion depth is bounded consistently across owned and borrowed
  `Node` / `Value` deserializers, preventing a stack overflow on deeply nested
  caller-built merge chains.
- YAML 1.1 timestamps with more than nine fractional-second digits are truncated
  to nanosecond precision instead of rejected.
- Resolved a broad batch of parser, emitter, lossless-editing, numeric-parsing,
  and diagnostic correctness issues (#19–#40), including a UTF-8 block-scalar
  boundary panic.

### Security

- Fixed three quadratic-time parsing denial-of-service vectors that were reachable
  under the default limits: unterminated multi-line flow collections, unterminated
  multi-line quoted scalars, and long single-line runs of quote characters. Each
  now scans in linear time.

## 0.2.0

### Added

- Added `ConfigEditor` and `ConfigPath` as a high-level config refactoring API
  for sequential path-based set/remove/rename/insert edits that preserve
  comments, anchors, ordering, and untouched bytes through the existing lossless
  graph editor.

### Fixed

- Preserved mapping and sequence types across chained high-level edits, including
  compact sequence-item mappings, generated collection fragments, custom
  `LoadOptions`, and empty mapping/sequence removals.

## 0.1.1

### Changed

- Removed the deprecated `serde_yml` and the `serde_yaml_ng` fork from the
  dev-dependency set and the compatibility matrix. Runtime dependencies are
  unchanged (`ryu`, `serde`); this only narrows the internal comparison set.

## 0.1.0

Initial `saneyaml` release.

### Added

- Package metadata, MIT license text, public docs, issue templates, pull
  request template, and CI.
- Serde-compatible read and write APIs for common config-shaped YAML,
  including `from_*`, `to_*`, `Value`, `Mapping`, `Number`, `Deserializer`,
  `Serializer`, and selected `serde_yaml::with` helper paths.
- YAML 1.2-oriented default loading with explicit Core, JSON, Failsafe, and
  LegacySerdeYaml schema modes.
- Pull-based event and document streams, source-backed lossless graph parsing,
  path-addressed lossless edits, structural emission, byte-compatible emission
  for a documented corpus, and span-preserving diagnostics.
- YAML test-suite, real-world fixture, downstream package-alias, fuzz,
  property, compatibility, and benchmark proof artifacts.

### Security

- Default 64 MiB input ceiling, input-derived alias expansion budget, recursive
  alias rejection, protective scalar/collection structural parser limits, ten
  fuzz targets, and release fuzz-sweep tooling.
