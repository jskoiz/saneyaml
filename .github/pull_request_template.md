## Summary

-

## Evidence

- [ ] `cargo fmt --all --check`
- [ ] `git diff --check`
- [ ] `cargo test --locked`
- [ ] `cargo clippy --locked --all-targets -- -D warnings`
- [ ] `RUSTDOCFLAGS='-D missing_docs' cargo doc --locked --no-deps`
- [ ] `cargo test --locked --doc`
- [ ] `scripts/check-public-api.sh`
- [ ] `cargo test --locked --test runtime_dependency_closure`
- [ ] `cargo test --locked --test trust_metadata`

## Compatibility and Release Notes

- [ ] Public API drift is intentional and documented, or `PUBLIC_API.txt` is unchanged.
- [ ] Runtime dependencies remain limited to direct `ryu` and `serde`.
- [ ] `CHANGELOG.md`, `COMPATIBILITY.md`, and `RELEASE_NOTES.md` are updated when claims changed.
- [ ] No hosted macOS/Windows workflow run was triggered without explicit maintainer approval.
