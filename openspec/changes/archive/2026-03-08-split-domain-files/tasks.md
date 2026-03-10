## 1. Split domain/action/identity.rs — extract Specifier

- [x] 1.1 Create `src/domain/action/specifier.rs` with `Specifier` enum, all its `impl` blocks, trait impls (`Display`, `PartialEq`, `Eq`, `Hash`, `From`), and associated tests
- [x] 1.2 Update `src/domain/action/identity.rs` to remove Specifier, add `use super::specifier::Specifier` or re-export
- [x] 1.3 Update `src/domain/action/mod.rs` to declare and re-export `specifier` module
- [x] 1.4 Verify `cargo test` passes and both files are under 500 lines

## 2. Extract tag selection helpers to domain/action/

- [x] 2.1 Create `src/domain/action/tag_selection.rs` with `select_most_specific_tag`, `parse_version_components`, and `ShaIndex` + their tests
- [x] 2.2 Update `src/domain/resolution.rs` to import from `action::tag_selection` instead of local definitions
- [x] 2.3 Update `src/domain/action/mod.rs` to declare and re-export `tag_selection` module
- [x] 2.4 Verify `cargo test` passes and `resolution.rs` is under 500 lines

## 3. Convert domain/manifest.rs → domain/manifest/ directory

- [x] 3.1 Create `src/domain/manifest/` directory with `mod.rs` and `overrides.rs`
- [x] 3.2 Move `ActionOverride`, `resolve_version()`, `sync_overrides()`, `prune_stale_overrides()`, `replace_overrides()`, `add_override()` and their tests to `overrides.rs`
- [x] 3.3 Keep `Manifest` struct, basic CRUD, `diff()`, `lock_keys()`, `specs()` and their tests in `mod.rs`
- [x] 3.4 Re-export `ActionOverride` and override methods from `mod.rs`
- [x] 3.5 Update `src/domain/mod.rs` — change `pub mod manifest;` (file) to work with the directory
- [x] 3.6 Verify `cargo test` passes and both files are under 500 lines

## 4. Convert domain/lock.rs → domain/lock/ directory

- [x] 4.1 Create `src/domain/lock/` directory with `mod.rs` and `entry.rs`
- [x] 4.2 Move `LockEntry` struct, `is_complete()`, `set_version()`, `set_comment()`, `with_version_and_comment()`, constructors, and their tests to `entry.rs`
- [x] 4.3 Keep `Lock` struct, CRUD, `diff()`, `build_update_map()`, `retain()`, `entries()` and their tests in `mod.rs`
- [x] 4.4 Re-export `LockEntry` from `mod.rs`
- [x] 4.5 Verify `cargo test` passes and both files are under 500 lines

## 5. Final verification

- [x] 5.1 Verify `src/domain/` direct `.rs` file count is ≤ 8
- [x] 5.2 Verify no files in `src/domain/` exceed 500 lines
- [x] 5.3 Run `cargo test` and `cargo clippy` — all green
- [x] 5.4 Update the TODO comments in `tests/code_health.rs` to reflect completed splits
