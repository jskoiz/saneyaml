# Changelog

This changelog follows the shape of Keep a Changelog. The crate remains a local
developer preview with `publish = false`; this file records release-candidate
work and does not announce a crates.io release.

## Unreleased

### Added

- Trust and release-engineering documentation: `SECURITY.md`,
  `CONTRIBUTING.md`, issue templates, pull request template, MSRV/stability
  policy text, public API drift checks, runtime dependency closure checks, and
  README local-status badges.
- CI configuration for Linux, macOS, Windows, Rust 1.85 MSRV proof, doctests,
  public API drift, runtime dependency closure, and template validation.

## 0.1.0 release-candidate work in progress

### Added

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

### Unreleased Blockers

- Public crate name, final license, version decision, publication identity, and
  crates.io publication still require explicit approval.
