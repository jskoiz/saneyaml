#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

expected_version="cargo-public-api 0.52.0"
actual_version="$(cargo public-api --version 2>/dev/null || true)"
if [[ "$actual_version" != "$expected_version" ]]; then
  echo "expected $expected_version, found: ${actual_version:-not installed}" >&2
  echo "install with: cargo install cargo-public-api --version 0.52.0 --locked" >&2
  exit 1
fi

nightly_cargo="$(rustup which --toolchain nightly cargo 2>/dev/null || true)"
if [[ -z "$nightly_cargo" ]]; then
  echo "nightly Rust toolchain is required for rustdoc JSON" >&2
  echo "install with: rustup toolchain install nightly --profile minimal" >&2
  exit 1
fi

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/yaml-public-api.XXXXXX")"
trap 'rm -rf "$tmpdir"' EXIT

nightly_bin="$(dirname "$nightly_cargo")"
PATH="$nightly_bin:$PATH" \
  CARGO_TARGET_DIR="$tmpdir/target" \
  cargo public-api --manifest-path Cargo.toml -sss --color never \
  > "$tmpdir/PUBLIC_API.txt"

diff -u docs/PUBLIC_API.txt "$tmpdir/PUBLIC_API.txt"
