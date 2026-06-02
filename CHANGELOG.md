# Changelog

This changelog follows the shape of Keep a Changelog.

## Unreleased

No changes yet.

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
