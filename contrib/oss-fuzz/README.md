# OSS-Fuzz integration (staging copy)

This directory is a ready-to-submit [OSS-Fuzz](https://github.com/google/oss-fuzz)
project definition for saneyaml. The canonical copy lives in the
`google/oss-fuzz` repository once accepted; this staging copy is kept in-tree
so changes to the fuzz harness and the OSS-Fuzz config can be reviewed
together.

It builds all 11 libFuzzer targets under [`fuzz/`](../../fuzz) with debug
assertions enabled (the targets assert parser invariants, not just
crash-freedom) and ships each target's checked-in corpus as its seed corpus.

## Submitting

1. Fork and clone `https://github.com/google/oss-fuzz`.
2. Copy this directory to `projects/saneyaml/` in the fork.
3. Verify locally (requires Docker):

   ```sh
   python infra/helper.py build_image saneyaml
   python infra/helper.py build_fuzzers --sanitizer address saneyaml
   python infra/helper.py check_build saneyaml
   python infra/helper.py run_fuzzer saneyaml parse_bytes -- -max_total_time=60
   ```

4. Open a PR titled `[saneyaml] Initial integration`. In the description,
   cover what OSS-Fuzz acceptance reviewers ask about: what the project is, who
   uses it (YAML parsing of untrusted input; serde_yaml replacement path), and
   that the primary contact is the maintainer.

## Notes

- `primary_contact` in `project.yaml` must be an email associated with a
  Google account to receive ClusterFuzz crash reports and access
  https://oss-fuzz.com. Additional maintainers can be added later via
  `auto_ccs`.
- The Dockerfile clones `main`, so OSS-Fuzz always fuzzes the current default
  branch; no release coupling.
- New fuzz targets added under `fuzz/fuzz_targets/` are picked up
  automatically by `cargo fuzz list` in `build.sh`.
- OSS-Fuzz requires the Apache-2.0 header on `Dockerfile` and `build.sh`
  (Google copyright line included per their contribution guidelines); it does
  not affect saneyaml's MIT licensing.
