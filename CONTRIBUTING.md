# Contributing

This repository is a package-ready candidate awaiting explicit publish
approval. Contributions should keep behavioral claims tied to reproducible local
evidence and should not publish, push tags, or run hosted CI without maintainer
approval.

## Local Setup

Use the repository root as the working directory. Before making changes, verify:

```sh
pwd
git rev-parse --show-toplevel
git branch --show-current
git status -sb
```

The crate MSRV is Rust 1.88. The active local cargo may be newer, so MSRV checks
should use:

```sh
rustup run 1.88.0 cargo check --locked --all-targets
rustup run 1.88.0 cargo test --locked
```

## Required Evidence

Run the strongest relevant subset for the files you changed:

```sh
cargo fmt --all --check
git diff --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --locked --no-deps
cargo test --locked --doc
scripts/check-public-api.sh
cargo test --locked --test runtime_dependency_closure
cargo test --locked --test trust_metadata
```

Parser, emitter, Serde, compatibility, fuzz, and fixture changes need targeted
tests in addition to the general stack. Security-sensitive fixes should include
the smallest safe regression artifact and should follow `SECURITY.md`.


## Public API and Stability

The pre-1.0 preview surface is documented in `COMPATIBILITY.md`. Public exports, public enum variants, public struct fields,
and public constants are SemVer-visible. If a change would alter the public API
snapshot, update `PUBLIC_API.txt` only when the API change is intentional and
documented.

Runtime dependencies remain limited to direct `ryu` and `serde`; any resolved
no-dev dependency tree change must update the snapshot and explain why it is
safe.

## Hosted CI

The workflow includes hosted Linux, Windows, and macOS jobs. Do not trigger
hosted macOS/Windows runs from this repository or a personal fork unless a
maintainer explicitly approves that exact repo, workflow, and ref run.
