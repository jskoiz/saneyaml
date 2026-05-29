#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runs="${YAML_FUZZ_RUNS:-1000}"
tmp="${TMPDIR:-/tmp}/yaml-fuzz-proof.$$"
targets=(parse_bytes serde_entrypoints event_stream emit_roundtrip apply_merge schema_modes lossless_graph lossless_edit)

nightly_cargo=""
if [[ -n "${YAML_NIGHTLY_BIN:-}" && -x "$YAML_NIGHTLY_BIN/cargo" ]]; then
  nightly_cargo="$YAML_NIGHTLY_BIN/cargo"
fi
if [[ -z "$nightly_cargo" ]] && command -v rustup >/dev/null 2>&1; then
  nightly_cargo="$(rustup which --toolchain nightly cargo 2>/dev/null || true)"
fi
if [[ -z "$nightly_cargo" && -x "$HOME/.rustup/toolchains/nightly-aarch64-apple-darwin/bin/cargo" ]]; then
  nightly_cargo="$HOME/.rustup/toolchains/nightly-aarch64-apple-darwin/bin/cargo"
fi

if [[ -n "$nightly_cargo" ]]; then
  export PATH="$(dirname "$nightly_cargo"):$PATH"
fi

cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT

mkdir -p "$tmp/corpus" "$tmp/artifacts" "$tmp/target"

for target in "${targets[@]}"; do
  cp -R "$repo_root/fuzz/corpus/$target" "$tmp/corpus/$target"
  mkdir -p "$tmp/artifacts/$target"
  (
    cd "$repo_root"
    cargo fuzz run --target-dir "$tmp/target" "$target" "$tmp/corpus/$target" -- \
      -runs="$runs" \
      -artifact_prefix="$tmp/artifacts/$target/"
  )
done
