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
    cargo package --locked --allow-dirty --target-dir "$tmp/package-target" >/dev/null
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

run_real_world_package_alias_smoke() {
  local smoke="$tmp/serde-yaml-real-world-alias-smoke"
  cp -R "$repo_root/tests/fixtures/downstream/package-alias-real-world-smoke" "$smoke"
  mkdir -p "$smoke/fixtures"
  cp -R "$repo_root/tests/fixtures/real-world" "$smoke/fixtures/real-world"
  mv "$smoke/Cargo.toml.template" "$smoke/Cargo.toml"
  YAML_PACKAGE_DIR="$package_dir" perl -0pi -e \
    's#__YAML_PACKAGE_DIR__#$ENV{YAML_PACKAGE_DIR}#g' \
    "$smoke/Cargo.toml"
  cargo run --manifest-path "$smoke/Cargo.toml" --quiet
}

run_external_downstream_package_alias_smoke() {
  local smoke="$tmp/serde-yaml-external-downstream-alias-smoke"
  cp -R "$repo_root/tests/fixtures/downstream/package-alias-external-downstream-smoke" "$smoke"
  mkdir -p "$smoke/fixtures"
  cp -R "$repo_root/tests/fixtures/downstream/pingora" "$smoke/fixtures/pingora"
  cp -R "$repo_root/tests/fixtures/downstream/rust-i18n" "$smoke/fixtures/rust-i18n"
  cp -R "$repo_root/tests/fixtures/downstream/cfn-guard" "$smoke/fixtures/cfn-guard"
  cp -R "$repo_root/tests/fixtures/downstream/navi" "$smoke/fixtures/navi"
  cp -R "$repo_root/tests/fixtures/downstream/stackable-operator" "$smoke/fixtures/stackable-operator"
  mv "$smoke/Cargo.toml.template" "$smoke/Cargo.toml"
  YAML_PACKAGE_DIR="$package_dir" perl -0pi -e \
    's#__YAML_PACKAGE_DIR__#$ENV{YAML_PACKAGE_DIR}#g' \
    "$smoke/Cargo.toml"
  cargo run --manifest-path "$smoke/Cargo.toml" --quiet
}

registry_source_dir() {
  local crate="$1"
  local version="$2"
  local cargo_home="${CARGO_HOME:-$HOME/.cargo}"
  local source

  source="$(find "$cargo_home/registry/src" -maxdepth 2 -type d -name "$crate-$version" 2>/dev/null | sort | tail -n 1 || true)"
  if [[ -z "$source" ]]; then
    local fetch="$tmp/fetch-$crate"
    mkdir -p "$fetch/src"
    cat >"$fetch/Cargo.toml" <<EOF
[package]
name = "yaml-downstream-fetch-$crate"
version = "0.0.0"
edition = "2021"

[dependencies]
$crate = "=$version"
EOF
    : >"$fetch/src/lib.rs"
    cargo fetch --manifest-path "$fetch/Cargo.toml" --quiet
    source="$(find "$cargo_home/registry/src" -maxdepth 2 -type d -name "$crate-$version" 2>/dev/null | sort | tail -n 1 || true)"
  fi

  if [[ -z "$source" ]]; then
    echo "could not find crates.io source for $crate $version" >&2
    return 1
  fi

  printf '%s\n' "$source"
}

copy_crates_io_checkout() {
  local crate="$1"
  local version="$2"
  local checkout="$3"
  local source

  source="$(registry_source_dir "$crate" "$version")"
  rm -rf "$checkout"
  cp -R "$source" "$checkout"
  chmod -R u+w "$checkout"
}

patch_serde_yaml_dependency() {
  local manifest="$1"
  YAML_PACKAGE_DIR="$package_dir" perl -0pi -e \
    's{^(\[[^\]\n]*(?:dependencies|dev-dependencies|build-dependencies)(?:\.[^\]\n]+)?\.serde_yaml\]\n)(.*?)(?=^\[|\z)}{
      my ($header, $body) = ($1, $2);
      my $optional = $body =~ /^\s*optional\s*=\s*true\s*$/m ? qq{optional = true\n} : "";
      qq{$header} . qq{package = "yaml"\npath = "$ENV{YAML_PACKAGE_DIR}"\n$optional\n}
    }egmsx;
    s{^([[:blank:]]*)serde_yaml[[:blank:]]*=[[:blank:]]*(?:"[^"]+"|\{[^\n]*\})}{
      my ($indent, $entry) = ($1, $&);
      my $optional = $entry =~ /optional\s*=\s*true/ ? ", optional = true" : "";
      qq{${indent}serde_yaml = { package = "yaml", path = "$ENV{YAML_PACKAGE_DIR}"$optional }}
    }egm' \
    "$manifest"
  if ! grep -q 'serde_yaml = { package = "yaml"' "$manifest" \
    && ! grep -q 'package = "yaml"' "$manifest"; then
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

run_navi_trial() {
  local checkout="$tmp/navi"
  git clone --quiet https://github.com/denisidoro/navi.git "$checkout"
  git -C "$checkout" checkout --quiet 1ac218cb1e0e80649ef23c8a916e67efc3086833
  patch_serde_yaml_dependency "$checkout/Cargo.toml"

  cargo check --manifest-path "$checkout/Cargo.toml" --lib
  cargo check --manifest-path "$checkout/Cargo.toml" --bin navi
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

run_figment_trial() {
  local checkout="$tmp/figment"
  copy_crates_io_checkout figment 0.10.19 "$checkout"
  patch_serde_yaml_dependency "$checkout/Cargo.toml"

  cargo check --manifest-path "$checkout/Cargo.toml" --features yaml
  cargo test --manifest-path "$checkout/Cargo.toml" --features yaml,test --test yaml-enum
}

run_uaparser_trial() {
  local checkout="$tmp/uaparser"
  copy_crates_io_checkout uaparser 0.6.4 "$checkout"
  patch_serde_yaml_dependency "$checkout/Cargo.toml"

  (
    cd "$checkout"
    cargo test --lib
    cargo check --examples
  )
}

package_current_crate
run_packaged_smoke
run_strict_package_alias_smoke
run_real_world_package_alias_smoke
run_external_downstream_package_alias_smoke

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
  navi)
    run_navi_trial
    ;;
  stackable-operator)
    run_stackable_operator_trial
    ;;
  figment)
    run_figment_trial
    ;;
  uaparser)
    run_uaparser_trial
    ;;
  smoke-only)
    ;;
  *)
    echo "unknown downstream build trial: $trial" >&2
    echo "available trials: pingora, rust-i18n, cfn-guard, navi, stackable-operator, figment, uaparser, smoke-only" >&2
    exit 2
    ;;
esac
