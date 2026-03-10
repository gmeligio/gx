## 1. Split infra/manifest.rs into manifest/ directory

- [x] 1.1 Create `src/infra/manifest/` directory with `mod.rs`, `convert.rs`, `patch.rs`
- [x] 1.2 Move TOML wire types (`ManifestData`, `GxSection`, `TomlOverride`, `TomlActions`, `LintData`) and conversion functions (`manifest_from_data`, `manifest_to_data`, `format_manifest_toml`) to `convert.rs`
- [x] 1.3 Move `apply_manifest_diff` and override helpers (`override_entry_matches`, `apply_override_removals`, `collect_override_removal_indices`, `apply_override_additions`) to `patch.rs`
- [x] 1.4 Keep `ManifestError`, `FileManifest`, `parse_manifest`, `create_manifest`, `parse_lint_config` in `mod.rs` with re-exports from submodules
- [x] 1.5 Distribute tests to their respective modules (conversion tests → `convert.rs`, patch tests → `patch.rs`, remaining → `tests.rs`)
- [x] 1.6 Verify `cargo test` passes and no file exceeds 500 lines

## 2. Split infra/lock.rs into lock/ directory

- [x] 2.1 Create `src/infra/lock/` directory with `mod.rs`, `convert.rs`
- [x] 2.2 Move wire types (`ActionEntryData`, `LockData`), conversion functions (`lock_from_data`, `serialize_lock`, `build_lock_inline_table`) to `convert.rs`
- [x] 2.3 Keep `LockFileError`, `FileLock`, `parse_lock`, `create_lock`, `apply_lock_diff` in `mod.rs`
- [x] 2.4 Distribute tests to their respective modules
- [x] 2.5 Verify `cargo test` passes and no file exceeds 500 lines

## 3. Split infra/github.rs into github/ directory

- [x] 3.1 Create `src/infra/github/` directory with `mod.rs`, `responses.rs`, `resolve.rs`
- [x] 3.2 Move all DTO structs (`GitRef`, `GitObject`, `GitRefEntry`, `CommitResponse`, `GitTagResponse`, `ReleaseResponse`, `CommitDetailResponse`, `CommitObject`, `CommitterInfo`, `TagObjectResponse`, `TaggerInfo`) to `responses.rs`
- [x] 3.3 Move resolution logic (`resolve_ref`, `fetch_ref`, `fetch_commit_sha`, `get_tags_for_sha`, `dereference_tag`, `get_version_tags`, `fetch_commit_date`, `fetch_release_date`, `fetch_tag_date`, `resolve_version_for_sha`, `filter_refs_by_sha`, `parse_next_link`) to `resolve.rs`
- [x] 3.4 Keep `GithubError`, `GithubRegistry` struct/constructor/HTTP helpers, `VersionRegistry` impl, `Default` impl in `mod.rs`
- [x] 3.5 Distribute tests to their respective modules
- [x] 3.6 Verify `cargo test` passes and no file exceeds 500 lines

## 4. Slim infra/workflow_scan.rs

- [x] 4.1 Extract the `#[cfg(test)] mod tests` block (~347 lines) from `workflow_scan.rs` into a separate test submodule or trim test helpers to bring the file under 500 lines
- [x] 4.2 Verify `cargo test` passes and no file exceeds 500 lines

## 5. Update re-exports and final verification

- [x] 5.1 Update `src/infra/mod.rs` for the new directory layout (manifest/, lock/, github/)
- [x] 5.2 Verify no other files in the codebase have broken imports
- [x] 5.3 Run `cargo test` and `cargo clippy` — all green
