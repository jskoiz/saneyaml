#!/usr/bin/env bash
set -euo pipefail

cargo clippy --locked --no-default-features --lib -- -D warnings

cargo clippy --locked --no-default-features --features lossless --all-targets -- -D warnings
