# Road to 1.0

saneyaml is pre-1.0 (`0.3.x`). The public API is treated as SemVer-visible
today: breaking changes ship only in `0.minor` bumps and are called out in the
[changelog](../CHANGELOG.md). The road to 1.0 is about locking the surface
down, not expanding it.

## What 1.0 means here

Releasing 1.0 is a commitment, not a milestone badge:

- **No breaking API changes** outside a 2.0, including diagnostics text
  guarded by tests and the documented `serde_yaml` rename-compatibility
  surface in [COMPATIBILITY.md](COMPATIBILITY.md).
- **MSRV policy**: MSRV bumps are minor-version events, documented in the
  changelog, and never exceed the oldest Rust release of the prior six
  months.
- **Deprecation policy**: anything removed in a future 2.0 is marked
  `#[deprecated]` for at least one minor release first.

## Gates

Each gate is a verifiable artifact, not an intention:

| gate | status |
|---|---|
| Full yaml-test-suite imported and classified (402/402) | done — [conformance](conformance.md) |
| Public conformance matrix vs other crates | done — [matrix](https://jskoiz.github.io/saneyaml/conformance/index.html) |
| `cargo semver-checks` gating every PR against the published release | done — CI `semver` job |
| Public API snapshot tracked in-repo | done — [PUBLIC_API.txt](PUBLIC_API.txt) |
| Continuous fuzzing of all targets | staged — [contrib/oss-fuzz](https://github.com/jskoiz/saneyaml/tree/main/contrib/oss-fuzz), pending upstream acceptance |
| API review pass over every `pub` item (naming, sealedness, `#[non_exhaustive]`) | open |
| Real-world migration validation (downstream crates' suites run against saneyaml) | in progress — five crates verified clean |
| `wasm32-unknown-unknown` library build kept green | done — CI `wasm` job |

When the open gates close, the then-current `0.x` is re-tagged 1.0-rc with a
soak window for migration feedback, then released as 1.0 unchanged unless an
rc bug forces a change.

## What 1.0 does not promise

- Byte-identical emitter output across versions outside the documented
  `byte_compatible()` corpus.
- Stability of `#[doc(hidden)]` items, including `__unstable_event_serde`.
- Wall-clock or resident-memory guarantees from the resource limits (see
  [Untrusted input](untrusted-input.md)).
