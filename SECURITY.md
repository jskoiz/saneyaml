# Security Policy

## Supported Versions

`yaml` is a local developer preview and is not published to crates.io. Until a
public release is approved, security triage applies to the current repository
`main` line only. There are no public backport or support branches yet.

## Reporting a Vulnerability

Use GitHub private vulnerability reporting for `jskoiz/yaml` when available. If
private reporting is unavailable, contact the maintainer privately on GitHub
before filing a public issue. Public issues should not include exploit payloads,
private configuration files, credentials, minimized denial-of-service payloads,
or unreduced crash inputs that could be directly reused against another
project.

For non-sensitive parser bugs, compatibility divergences, and documentation
issues, use the public issue templates.

## Resource-Limit Posture

The default loader posture is bounded for untrusted YAML inputs:

- `LoadOptions` applies a default 64 MiB input byte ceiling.
- Callers can tune `max_input_bytes()` or explicitly opt out with
  `without_input_limit()` only after bounding the source themselves.
- Alias expansion uses an input-derived budget by default and can be tuned with
  `max_alias_expansion_nodes()`.
- Recursive aliases are rejected.
- Default nesting, scalar-size, and collection-item limits protect parser,
  loader, Serde, and lossless entrypoints from unbounded structural work:
  128 constructed nesting levels, 1 MiB resolved scalars, and 16,384 entries per
  sequence or mapping.

Reader-backed entrypoints still fully buffer the bounded input before parsing.
The limits are parser and construction safety controls; they are not a
wall-clock, resident-memory, or sandbox guarantee.

## Fuzzing and Regression Coverage

The repository carries ten fuzz targets, non-mutating corpus replay through
`scripts/fuzz-smoke-nonmutating.sh`, and a manual release sweep through
`scripts/fuzz-release-sweep.sh`. Corpus release floors and named safety seeds
are also gated by the property-test suite.

Security-relevant fixes should include the narrowest reproducible test,
fixture, corpus seed, or divergence record that proves the issue stays fixed
without disclosing sensitive payloads.
