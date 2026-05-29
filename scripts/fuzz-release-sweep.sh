#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runs="${YAML_FUZZ_RUNS:-10000}"
summary="${YAML_FUZZ_SUMMARY:-$repo_root/target/fuzz-release-sweep.md}"
artifact_root="${YAML_FUZZ_ARTIFACT_DIR:-$repo_root/target/fuzz-release-artifacts/$(date -u +%Y%m%dT%H%M%SZ)-$$}"
tmp="${TMPDIR:-/tmp}/yaml-fuzz-release-sweep.$$"
default_targets=(parse_bytes serde_entrypoints event_stream emit_roundtrip apply_merge schema_modes lossless_graph lossless_edit)

if [[ -n "${YAML_FUZZ_TARGETS:-}" ]]; then
  read -r -a targets <<< "$YAML_FUZZ_TARGETS"
else
  targets=("${default_targets[@]}")
fi

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

mkdir -p "$tmp/corpus" "$tmp/target" "$artifact_root" "$(dirname "$summary")"

{
  echo "# YAML Fuzz Release Sweep"
  echo
  echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "Repository: $repo_root"
  echo "Runs per target: $runs"
  if [[ -n "${YAML_FUZZ_SEED:-}" ]]; then
    echo "Seed: $YAML_FUZZ_SEED"
  else
    echo "Seed: libFuzzer default"
  fi
  echo "Artifact root: $artifact_root"
  echo
  echo "| Target | Corpus files | Runs | Status | Elapsed seconds | Artifact directory |"
  echo "|---|---:|---:|---|---:|---|"
} > "$summary"

overall_status=0
for target in "${targets[@]}"; do
  corpus_src="$repo_root/fuzz/corpus/$target"
  corpus_dst="$tmp/corpus/$target"
  artifacts="$artifact_root/$target"
  mkdir -p "$artifacts"

  if [[ ! -d "$corpus_src" ]]; then
    printf '| `%s` | 0 | `%s` | missing corpus | 0 | `%s` |\n' \
      "$target" "$runs" "$artifacts" >> "$summary"
    overall_status=1
    continue
  fi

  cp -R "$corpus_src" "$corpus_dst"
  corpus_count="$(find "$corpus_dst" -type f | wc -l | tr -d ' ')"
  start="$(date +%s)"
  set +e
  if [[ -n "${YAML_FUZZ_SEED:-}" ]]; then
    (
      cd "$repo_root"
      cargo fuzz run --target-dir "$tmp/target" "$target" "$corpus_dst" -- \
        -runs="$runs" \
        -artifact_prefix="$artifacts/" \
        "-seed=$YAML_FUZZ_SEED"
    )
  else
    (
      cd "$repo_root"
      cargo fuzz run --target-dir "$tmp/target" "$target" "$corpus_dst" -- \
        -runs="$runs" \
        -artifact_prefix="$artifacts/"
    )
  fi
  status=$?
  set -e

  elapsed="$(( $(date +%s) - start ))"
  if [[ "$status" -eq 0 ]]; then
    result="passed"
  else
    result="failed ($status)"
    if [[ "$overall_status" -eq 0 ]]; then
      overall_status="$status"
    fi
  fi
  printf '| `%s` | %s | `%s` | %s | %s | `%s` |\n' \
    "$target" "$corpus_count" "$runs" "$result" "$elapsed" "$artifacts" >> "$summary"
done

echo "fuzz release sweep summary: $summary"
cat "$summary"
exit "$overall_status"
