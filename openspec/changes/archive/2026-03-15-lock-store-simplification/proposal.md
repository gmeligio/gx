## Why

The lock file write path has a bug: format migration happens in memory but never persists to disk. `apply_lock_diff()` short-circuits on empty diffs, so the flat-to-two-tier conversion never completes. This causes `tidy` to print "migrated gx.lock -> v1.4" on every run without ever writing the new format.

The root cause is architectural: the lock uses diff-based patching (designed for human-edited files) on a machine-generated file. This complexity also carries legacy migration code for v1.0/v1.3 formats that no longer need support (~3 users total). Fixing the bug properly means simplifying the entire lock I/O layer.

## What Changes

- **BREAKING**: Drop migration support for v1.0 (plain SHA strings) and v1.3 (`@v6` style keys) lock formats. Users with these formats must delete `gx.lock` and run `gx tidy` to regenerate.
- Replace free functions (`parse()`, `create()`, `apply_lock_diff()`) with a `Store { load, save }` API. `save()` always writes the full lock in the current two-tier format.
- Remove the `Parsed<T>` wrapper and `migrated` flag from `Config`. Migration is transparent — any legacy format is read silently, and the next write produces the current format.
- Colocate the current format's serde structs, reader, and writer in `format.rs`. Legacy readers live in `migration.rs`.
- `tidy::plan()` and `upgrade::plan()` return the final `Lock` state alongside the diff. Callers write via `Store::save(planned_lock)` instead of re-applying diffs.
- Remove all "migrated gx.lock" progress messages from init, tidy, and upgrade commands.

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `file-format`: Remove v1.0, v1.3, v1.4 migration requirements. Simplify Format Migration section to cover only flat-to-two-tier migration. Remove migration messages. Add requirement that `Store::save()` always writes the current format (full rewrite, not diff-based patching).

## Impact

- **Code**: `src/infra/lock/` module restructured (mod.rs, format.rs, migration.rs). `src/config.rs` loses `lock_migrated` field. `src/tidy/mod.rs`, `src/upgrade/mod.rs`, `src/init/mod.rs` switch from `apply_lock_diff`/`create` to `Store::save`. `src/domain/mod.rs` loses `Parsed<T>`.
- **Tests**: Lock migration tests for v1.0/v1.3 deleted. `apply_lock_diff` tests rewritten as `Store::save` tests. Integration tests updated to use `Store` API.
- **Breaking**: Users on v1.0/v1.3 lock formats must regenerate. Acceptable given ~3 total users.
