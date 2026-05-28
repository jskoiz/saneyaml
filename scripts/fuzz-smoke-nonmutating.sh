#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runs="${YAML_FUZZ_RUNS:-1000}"
tmp="${TMPDIR:-/tmp}/yaml-fuzz-proof.$$"
targets=(parse_bytes serde_entrypoints event_stream apply_merge schema_modes lossless_graph lossless_edit)
nightly_bin="${YAML_NIGHTLY_BIN:-$HOME/.rustup/toolchains/nightly-aarch64-apple-darwin/bin}"

if [[ -x "$nightly_bin/cargo" ]]; then
  export PATH="$nightly_bin:$PATH"
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
