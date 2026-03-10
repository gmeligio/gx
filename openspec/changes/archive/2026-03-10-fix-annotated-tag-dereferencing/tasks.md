## 1. Fix annotated tag dereferencing

- [x] 1.1 Rename `fetch_ref` to `fetch_ref_commit` in `src/infra/github/resolve.rs` and update both call sites in `resolve_ref` (tag path line 41, branch path line 56)
- [x] 1.2 Add annotated tag dereferencing in `fetch_ref_commit`: after parsing `GitRef`, check `object.type == "tag"` and dereference via `GET /git/tags/{sha}` to return the underlying commit SHA

## 2. Remove dead code

- [x] 2.1 Delete `resolve_version_for_sha` method from `src/infra/github/resolve.rs`
- [x] 2.2 Delete `test_resolve_version_for_sha_no_matching_tags` from `src/infra/github/tests.rs`
- [x] 2.3 Delete `test_resolve_version_for_sha_matches_annotated_tags` from `tests/e2e_github.rs`

## 3. Clean up public re-exports

- [x] 3.1 Remove `pub use responses::{GitObject, GitRef, GitRefEntry}` from `src/infra/github/mod.rs` (change to `pub(super)` visibility on the response structs or remove the re-export line)

## 4. Verify

- [x] 4.1 Run `cargo check` — no compile errors
- [x] 4.2 Run `cargo test` (unit + integration) — all pass
- [x] 4.3 Run e2e tests (`test_resolve_ref_annotated_tag_returns_commit_sha_not_tag_object`, `e2e_init_annotated_tag_action_produces_valid_commit_sha`) — both pass
