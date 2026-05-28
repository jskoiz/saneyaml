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
  mkdir -p "$smoke/src"
  cat >"$smoke/Cargo.toml" <<EOF
[package]
name = "serde-yaml-alias-smoke"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_yaml = { package = "yaml", path = "$package_dir" }
EOF
  cat >"$smoke/src/main.rs" <<'EOF'
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Config {
    name: String,
    ports: Vec<u16>,
}

fn main() {
    let config: Config = serde_yaml::from_str("name: api\nports: [80, 443]\n").unwrap();
    assert_eq!(config.name, "api");
    assert_eq!(config.ports, [80, 443]);

    let value: serde_yaml::Value =
        serde_yaml::from_str("defaults: &defaults\n  retries: 3\njob:\n  <<: *defaults\n").unwrap();
    assert_eq!(value["job"]["retries"].as_u64(), Some(3));
}
EOF
  cargo check --manifest-path "$smoke/Cargo.toml" --quiet
}

patch_workspace_serde_yaml_dependency() {
  local manifest="$1"
  YAML_PACKAGE_DIR="$package_dir" perl -0pi -e \
    's/^serde_yaml\s*=\s*"[^"]+"/serde_yaml = { package = "yaml", path = "$ENV{YAML_PACKAGE_DIR}" }/m' \
    "$manifest"
  if ! grep -q 'serde_yaml = { package = "yaml"' "$manifest"; then
    echo "failed to rewrite serde_yaml workspace dependency in $manifest" >&2
    return 1
  fi
}

run_rust_i18n_trial() {
  local checkout="$tmp/rust-i18n"
  git clone --quiet https://github.com/longbridge/rust-i18n.git "$checkout"
  git -C "$checkout" checkout --quiet 97cf091c24e4bc09a0acb397a8d9d7da8b6abc56
  patch_workspace_serde_yaml_dependency "$checkout/Cargo.toml"

  cargo check --manifest-path "$checkout/Cargo.toml" -p rust-i18n-support --features codegen
  cargo check --manifest-path "$checkout/Cargo.toml" -p rust-i18n-macro
  cargo check --manifest-path "$checkout/Cargo.toml" -p rust-i18n-extract
}

package_current_crate
run_packaged_smoke

case "$trial" in
  rust-i18n)
    run_rust_i18n_trial
    ;;
  smoke-only)
    ;;
  *)
    echo "unknown downstream build trial: $trial" >&2
    echo "available trials: rust-i18n, smoke-only" >&2
    exit 2
    ;;
esac
