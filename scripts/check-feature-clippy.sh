#!/usr/bin/env bash
set -euo pipefail

cargo clippy --locked --no-default-features --lib -- -D warnings

for features in serde emit serde,emit; do
    cargo clippy --locked --no-default-features --features "$features" --lib -- -D warnings
done

for features in lossless serde,lossless emit,lossless; do
    cargo clippy --locked --no-default-features --features "$features" --all-targets -- -D warnings
done
