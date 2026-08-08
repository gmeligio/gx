## 1. Baseline

- [x] 1.1 Record the pre-split behavior baseline: `mise run test` passes and `tests/code_health.rs` is unmodified in the working tree
- [x] 1.2 Confirm the baseline line-count table in `design.md` (Context) still matches `src/infra/github/*.rs` on disk; the post-split counts get reported against it in 5.4

## 2. Shared request helper (Decision 1)

- [x] 2.1 Add `pub(super) fn get_json<T: DeserializeOwned>(&self, url: &str, operation: &'static str) -> Result<T, Error>` to `src/infra/github/registry.rs`, beside `authenticated_get` and `check_status`, with a doc comment (`missing_docs_in_private_items` is denied)
- [x] 2.2 Confirm the helper calls `check_status` before `json()` — parsing a non-2xx body would turn `NotFound` into `ParseResponse` and break `resolve_ref`'s fallback chain
- [x] 2.3 Leave `dereference_tag` and `get_version_tags` OFF the helper — they are the two exempt call sites (Decision 1); routing either through `get_json` changes behavior
- [x] 2.4 Confirm `registry.rs` stays under the 440 logic-line / 550 total-line budgets after the addition

## 3. Split into three files (Decision 2)

- [x] 3.1 Widen `GITHUB_API_BASE` in `resolve.rs` to `pub(super)` so the sibling files can import it (Decision 3)
- [x] 3.2 Leave `resolve_ref`, `fetch_ref_commit`, and `fetch_commit_sha` in `resolve.rs`; route their requests through `get_json`, preserving each `operation` string exactly (`"ref"`, `"tag dereference"`, `"commit"`)
- [x] 3.3 Keep `fetch_ref_commit`'s owner/repo re-extraction from the ref URL verbatim — it is the annotated-tag dereference path and is the highest-risk move
- [x] 3.4 Create `src/infra/github/tags.rs` with `get_tags_for_sha`, `dereference_tag`, `get_version_tags`, `parse_next_link`, and `filter_refs_by_sha`, preserving the `"tags"` and `"version tags"` operation strings
- [x] 3.5 Keep `get_version_tags`'s pagination on its own explicit request path — it reads response headers via `parse_next_link` before consuming the body, which `get_json` cannot express
- [x] 3.6 Keep `dereference_tag` returning `Option` and swallowing errors exactly as before (Observability: pre-existing silent path, preserved deliberately)
- [x] 3.7 Create `src/infra/github/dates.rs` with `fetch_commit_date`, `fetch_release_date`, and `fetch_tag_date`, preserving the `"commit details"`, `"release"`, and `"tag"` operation strings
- [x] 3.8 Declare `mod tags;` and `mod dates;` in `src/infra/github/mod.rs` with doc comments; leave `pub use registry::{Error, Registry};` unchanged

## 4. Lint and test-block hygiene

- [x] 4.1 Put `#[expect(clippy::multiple_inherent_impl, ...)]` on each `impl Registry` block that triggers the lint — the three split files, but not the first block in `registry.rs` (Decision 4)
- [x] 4.2 Split the existing `#[cfg(test)]` block: `full_sha_passthrough`, `subpath_action_extracts_base_repo`, and `version_resolver_trait` stay in `resolve.rs`; the three `filter_refs_by_sha` tests plus `make_ref_entry`/`make_ref_entry_typed` move to `tags.rs`
- [x] 4.3 Move test bodies verbatim — do not edit assertions
- [x] 4.4 Confirm every `#[cfg(test)]` block is at the bottom of its file with no top-level public item after it
- [x] 4.5 Confirm no new file uses a denied generic name (`types.rs`, `utils.rs`, `helpers.rs`, `common.rs`, `misc.rs`, `consts.rs`, `constants.rs`)

## 5. Verify

- [x] 5.1 Review the diff method-by-method against the pre-split file, checking URL template and `operation` string for each — these are unchecked string literals (Risks)
- [x] 5.2 Read `get_version_tags`'s pagination loop line-by-line against the original, especially the `match next_url { Some(next) => url = next, None => break }` branch — it is the largest hand-transcribed block and has no test at any level, so diff review is its only mitigation (critical path #1)
- [x] 5.3 Confirm each method's endpoint sequence and fallback order is unchanged, especially `resolve_ref`'s tag → release → branch → commit chain
- [x] 5.4 Run `mise run test` and confirm it passes with `tests/code_health.rs` unmodified
- [x] 5.5 Report post-split line counts for `src/infra/github/*.rs` against the baseline table in `design.md`; confirm the directory holds at most 8 `.rs` files and every file is under both budgets, with headroom for #137, #145, and #141
- [x] 5.6 Run `mise run integ`
- [x] 5.7 Run `tests/e2e_github.rs` with `GITHUB_TOKEN=$(gh auth token)` if a token is available; note it as skipped if not
- [x] 5.8 Confirm the only files modified are under `src/infra/github/` plus this change's own artifacts
