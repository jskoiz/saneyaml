# Changelog

This changelog follows the shape of Keep a Changelog.

Policy: user-visible changes land under `Unreleased` as they merge, grouped by
the standard `Added`/`Changed`/`Deprecated`/`Removed`/`Fixed`/`Security`
headings. On release, the `Unreleased` heading is renamed to the new version
number and a fresh, empty `Unreleased` section is added at the top. Entries
describe the human-visible change only; internal refactors with no observable
effect are omitted.

## Unreleased

### Added

- Added `ConfigEditor` and `ConfigPath` as a high-level config refactoring API
  for sequential path-based set/remove/rename/insert edits that preserve
  comments, anchors, ordering, and untouched bytes through the existing lossless
  graph editor.

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
