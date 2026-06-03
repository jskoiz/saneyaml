#!/usr/bin/env bash
set -euo pipefail

cargo clippy --locked --no-default-features -- -D warnings
cargo clippy --locked --no-default-features --features serde,emit -- -D warnings
