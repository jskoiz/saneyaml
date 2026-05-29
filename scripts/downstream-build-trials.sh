#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
trial="${1:-rust-i18n}"
tmp="${TMPDIR:-/tmp}/yaml-downstream-build-trial.$$"

cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT

package_current_crate() {
  mkdir -p "$tmp/package-target" "$tmp/unpacked"
  (
    cd "$repo_root"
    cargo package --allow-dirty --target-dir "$tmp/package-target" >/dev/null
  )

  local crate_path
  crate_path="$(find "$tmp/package-target/package" -maxdepth 1 -type f -name 'yaml-*.crate' | sort | tail -n 1)"
  if [[ -z "$crate_path" ]]; then
    echo "could not find packaged yaml crate" >&2
    return 1
  fi

  tar -xf "$crate_path" -C "$tmp/unpacked"
  package_dir="$(find "$tmp/unpacked" -maxdepth 1 -type d -name 'yaml-*' | sort | tail -n 1)"
  if [[ -z "$package_dir" ]]; then
    echo "could not find unpacked yaml package" >&2
    return 1
  fi
}

run_packaged_smoke() {
  local smoke="$tmp/serde-yaml-alias-smoke"
  cp -R "$repo_root/tests/fixtures/downstream/package-alias-smoke" "$smoke"
  mv "$smoke/Cargo.toml.template" "$smoke/Cargo.toml"
  YAML_PACKAGE_DIR="$package_dir" perl -0pi -e \
    's#__YAML_PACKAGE_DIR__#$ENV{YAML_PACKAGE_DIR}#g' \
    "$smoke/Cargo.toml"
  cargo run --manifest-path "$smoke/Cargo.toml" --quiet
}

run_strict_package_alias_smoke() {
  local fixture="$repo_root/tests/fixtures/downstream/package-alias-smoke-strict"

  local upstream="$tmp/serde-yaml-strict-upstream-smoke"
  cp -R "$fixture" "$upstream"
  cp "$upstream/Cargo.toml.upstream" "$upstream/Cargo.toml"
  cargo run --manifest-path "$upstream/Cargo.toml" --quiet

  local alias="$tmp/serde-yaml-strict-alias-smoke"
  cp -R "$fixture" "$alias"
  cp "$alias/Cargo.toml.template" "$alias/Cargo.toml"
  YAML_PACKAGE_DIR="$package_dir" perl -0pi -e \
    's#__YAML_PACKAGE_DIR__#$ENV{YAML_PACKAGE_DIR}#g' \
    "$alias/Cargo.toml"
  cargo run --manifest-path "$alias/Cargo.toml" --quiet
}

patch_serde_yaml_dependency() {
  local manifest="$1"
  YAML_PACKAGE_DIR="$package_dir" perl -0pi -e \
    's/^serde_yaml\s*=\s*"[^"]+"/serde_yaml = { package = "yaml", path = "$ENV{YAML_PACKAGE_DIR}" }/m' \
    "$manifest"
  if ! grep -q 'serde_yaml = { package = "yaml"' "$manifest"; then
    echo "failed to rewrite serde_yaml dependency in $manifest" >&2
    return 1
  fi
}

run_rust_i18n_trial() {
  local checkout="$tmp/rust-i18n"
  git clone --quiet https://github.com/longbridge/rust-i18n.git "$checkout"
  git -C "$checkout" checkout --quiet 97cf091c24e4bc09a0acb397a8d9d7da8b6abc56
  patch_serde_yaml_dependency "$checkout/Cargo.toml"

  cargo check --manifest-path "$checkout/Cargo.toml" -p rust-i18n-support --features codegen
  cargo check --manifest-path "$checkout/Cargo.toml" -p rust-i18n-macro
  cargo check --manifest-path "$checkout/Cargo.toml" -p rust-i18n-extract
}

run_cfn_guard_trial() {
  local checkout="$tmp/cfn-guard"
  git clone --quiet https://github.com/aws-cloudformation/cloudformation-guard.git "$checkout"
  git -C "$checkout" checkout --quiet ae35f4e6a5618ffb1f3653c084c450f82fc2fc51
  patch_serde_yaml_dependency "$checkout/guard/Cargo.toml"

  cargo check --manifest-path "$checkout/Cargo.toml" -p cfn-guard
}

run_pingora_trial() {
  local checkout="$tmp/pingora"
  git clone --quiet https://github.com/cloudflare/pingora.git "$checkout"
  git -C "$checkout" checkout --quiet c0845a8693b0792a6ccd0626e8475990f7269af2
  patch_serde_yaml_dependency "$checkout/pingora-core/Cargo.toml"
  patch_serde_yaml_dependency "$checkout/pingora-proxy/Cargo.toml"

  cargo check --manifest-path "$checkout/Cargo.toml" -p pingora-core
  cargo check --manifest-path "$checkout/Cargo.toml" -p pingora-proxy --example modify_response
}

run_stackable_operator_trial() {
  local checkout="$tmp/operator-rs"
  git clone --quiet https://github.com/stackabletech/operator-rs.git "$checkout"
  git -C "$checkout" checkout --quiet fd86c0ebf9b885be2684d7d867d513ab9df8c2e1
  patch_serde_yaml_dependency "$checkout/Cargo.toml"

  cargo check --manifest-path "$checkout/Cargo.toml" -p stackable-shared
  cargo test --manifest-path "$checkout/Cargo.toml" -p k8s-version --features serde --lib
}

package_current_crate
run_packaged_smoke
run_strict_package_alias_smoke

case "$trial" in
  pingora)
    run_pingora_trial
    ;;
  rust-i18n)
    run_rust_i18n_trial
    ;;
  cfn-guard)
    run_cfn_guard_trial
    ;;
  stackable-operator)
    run_stackable_operator_trial
    ;;
  smoke-only)
    ;;
  *)
    echo "unknown downstream build trial: $trial" >&2
    echo "available trials: pingora, rust-i18n, cfn-guard, stackable-operator, smoke-only" >&2
    exit 2
    ;;
esac
