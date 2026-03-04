## Why

After `describe-sha-trait-method`, each SHA is described efficiently in one registry call. But the same `(ActionId, CommitSha)` pair is still described multiple times across tidy phases because `ActionResolver` is stateless and created fresh per phase. The domain needs a structure that accumulates SHA knowledge during a plan run so the registry is only called once per unique SHA.

## What Changes

- Add `ShaIndex` domain entity that accumulates `ShaDescription` results keyed by `(ActionId, CommitSha)`
- `ShaIndex::get_or_describe()` checks existing knowledge before calling the registry — the single point of "do I already know this?"
- `ActionResolver` methods (`resolve_from_sha`, `correct_version`) accept `&mut ShaIndex` and delegate SHA knowledge to it instead of calling the registry directly for SHA operations
- Delete `refine_version` — dead code with no callers; `upgrade_sha_versions_to_tags` uses `ShaIndex` directly instead
- `ActionResolver` remains stateless — pure transformation logic
- `tidy::plan()` creates one `ShaIndex` and threads it through all phases, replacing per-phase resolver creation
- Phase functions (`sync_manifest_actions`, `upgrade_sha_versions_to_tags`, `update_lock`) accept `&ActionResolver<R>` + `&mut ShaIndex` instead of `&R`

## Capabilities

### New Capabilities

- `sha-index`: Domain entity that accumulates commit SHA descriptions during a plan run, providing deduplication across tidy phases.

### Modified Capabilities

- `sha-first-resolution`: `resolve_from_sha` and `correct_version` use `ShaIndex` for SHA knowledge instead of calling the registry directly. `refine_version` is deleted (dead code). `upgrade_sha_versions_to_tags` uses `ShaIndex` directly for tag lookup. External behavior is unchanged.

## Impact

- `src/domain/resolution.rs` — new `ShaIndex` struct, `ActionResolver` method signatures gain `&mut ShaIndex` parameter, `refine_version` deleted
- `src/commands/tidy.rs` — phase functions take resolver + sha_index instead of bare registry; single resolver created in `plan()`
- No infrastructure changes. Registry stays dumb.
