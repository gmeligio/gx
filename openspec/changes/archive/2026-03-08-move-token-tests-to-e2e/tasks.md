## 1. Create e2e test file

- [x] 1.1 Create `tests/e2e_github.rs` with the two moved tests (`test_resolve_ref_returns_release_for_tag_with_release` and `test_get_tags_for_sha_includes_annotated_tags`), a `github_registry()` helper, and necessary imports — without the runtime `GITHUB_TOKEN` guard

## 2. Remove tests from unit module

- [x] 2.1 Delete `test_resolve_ref_returns_release_for_tag_with_release` (L778-789) and `test_get_tags_for_sha_includes_annotated_tags` (L853-867) from `src/infra/github.rs` `mod tests`

## 3. Update mise task

- [x] 3.1 Update `[tasks.e2e]` in `.config/mise.toml` to use a for-loop pattern matching `mise run integ`: `cargo test $(for f in tests/e2e_*.rs; do echo --test $(basename $f .rs); done)`

## 4. Verify

- [x] 4.1 Run `mise run test` — unit tests pass, no GITHUB_TOKEN-dependent tests present
- [x] 4.2 Run `mise run clippy` — no warnings
