#!/bin/bash -eu
# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
################################################################################

cd "$SRC/saneyaml/fuzz"

# The fuzz targets assert parser invariants (round-trip stability, span
# bounds, limit enforcement), so debug assertions stay enabled in the
# optimized build.
cargo fuzz build -O --debug-assertions

for target in $(cargo fuzz list); do
  cp "target/x86_64-unknown-linux-gnu/release/$target" "$OUT/"
  if [ -d "corpus/$target" ]; then
    zip -j -q "$OUT/${target}_seed_corpus.zip" "corpus/$target"/*
  fi
done
