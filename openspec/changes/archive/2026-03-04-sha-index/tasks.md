# Tasks: sha-index

## Implementation Tasks

- [x] Add `ShaIndex` struct to `src/domain/resolution.rs` with `get_or_describe` method
- [x] Update `resolve_from_sha` to accept `&mut ShaIndex` and delegate to it
- [x] Update `correct_version` to accept `&mut ShaIndex` and delegate to it
- [x] Delete `refine_version` (dead code, no callers)
- [x] Update `upgrade_sha_versions_to_tags` to accept and use `ShaIndex` directly via `get_or_describe`
- [x] Update `tidy::plan()` to create one `ShaIndex` and thread it through all phases
- [x] Update `sync_manifest_actions` signature to accept `&ActionResolver<R>` + `&mut ShaIndex`
- [x] Update `update_lock` signature to accept `&ActionResolver<R>` + `&mut ShaIndex`
- [x] Export `ShaIndex` from `src/domain/mod.rs`
- [x] Update unit tests in `resolution.rs` to pass `ShaIndex` where needed
- [x] Update unit tests in `tidy.rs` to compile with new signatures
