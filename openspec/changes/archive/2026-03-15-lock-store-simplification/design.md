## Context

The lock file I/O layer (`src/infra/lock/`) currently exposes three free functions (`parse`, `create`, `apply_lock_diff`) and a `Store` struct with only `save`. The read path dispatches across four format versions (v1.0, v1.3, v1.4, two-tier) via version string matching and structural detection. The write path uses diff-based patching (`apply_lock_diff`), which re-reads the file from disk, applies a `LockDiff`, and writes back. A `Parsed<T>` wrapper carries a `migrated: bool` flag through `Config` into every command.

The `tidy::plan()` function computes the final planned `Lock` in memory, diffs it against the original, discards the planned lock, and returns only the diff. Callers then re-apply the diff via `apply_lock_diff`. When the diff is empty (no logical changes, just format migration), the write is skipped entirely.

## Goals / Non-Goals

**Goals:**
- Fix the migration-never-persists bug by making all write paths produce the current format unconditionally
- Simplify the lock I/O API to `Store { load, save }` — one struct, two methods
- Colocate the current format's read and write logic in a single file (`format.rs`)
- Isolate legacy format readers in `migration.rs` for easy addition/removal
- Remove the `migrated` flag and `Parsed<T>` wrapper from the codebase
- Return the planned `Lock` from `plan()` so callers write the final state directly

**Non-Goals:**
- Changing the two-tier lock format itself (the on-disk format stays the same)
- Changing how the manifest or workflow files are read/written
- Optimizing lock file write performance (full rewrite is fast enough)
- Supporting v1.0 or v1.3 lock formats (breaking change, ~3 users)

## Decisions

### 1. `Store` as the single public API

**Decision**: Replace `parse()`, `create()`, and `apply_lock_diff()` with `Store::load()` and `Store::save()`.

**Rationale**: Three free functions with overlapping responsibilities (`create` writes from a diff, `apply_lock_diff` reads+patches+writes, `parse` reads) create confusion about which to call when. A single struct with `load`/`save` makes the API self-documenting and eliminates the "which function do I use?" question.

**Alternative considered**: Keep free functions but add a `force_write` parameter to `apply_lock_diff`. Rejected because it patches over the design issue rather than fixing it, and the diff round-trip (plan computes final state → diffs → caller re-applies diff) is inherently fragile.

### 2. Full rewrite on save (no diff-based patching)

**Decision**: `Store::save()` always serializes the full `Lock` to disk. No diff-based patching.

**Rationale**: The lock file is machine-generated. Diff-based patching was designed for human-edited files where preserving formatting matters. For the lock file, it adds complexity without value and is the root cause of the migration bug. Cargo, npm, and yarn all do full rewrites. The output is deterministic (sorted by action ID), so git diffs remain clean.

**Alternative considered**: Keep diff-based patching but force a write when the format changes. Rejected because it requires carrying format state through the system and creates two write paths to maintain.

### 3. Colocated format module (`format.rs`)

**Decision**: The current format's serde structs, reader (`try_parse`), and writer (`write`) live together in `format.rs`.

**Rationale**: When the format is the same for reading and writing, colocation makes it easy to verify roundtrip correctness. When looking at `format.rs`, you see "this is what gx.lock looks like today" — the complete picture in one file.

**Migration lifecycle**: When the format changes in the future, move the current reader from `format.rs` into `migration.rs` (it becomes a legacy reader), then update `format.rs` with the new format's structs + reader + writer.

### 4. Structural format detection (no version strings)

**Decision**: Detect the format by structure (`content.contains("[resolutions")`) rather than by a version string field.

**Rationale**: The version string was already unreliable — the current flat format in the repo has no version field. Structural detection is what the code already does for the two-tier format. It's more robust (can't be wrong if the structure matches) and eliminates the need for a version field in the file.

### 5. Plan returns final `Lock` alongside diff

**Decision**: `tidy::Plan` and `upgrade::Plan` include the final `Lock` state. The `LockDiff` is retained for reporting only.

```rust
// tidy::Plan
pub struct Plan {
    pub manifest: ManifestDiff,
    pub lock: Lock,              // final state — written by Store::save()
    pub lock_changes: LockDiff,  // for reporting only
    pub workflows: Vec<WorkflowPatch>,
}
```

**Rationale**: The planned `Lock` is the source of truth. Currently it's computed inside `plan()` and discarded, then the diff is re-applied from disk — a lossy round-trip. Returning it directly eliminates the re-read and re-apply.

`is_empty()` on `Plan` should check `lock_changes.is_empty()` (not `lock.is_empty()`) to determine if there are reportable changes. The lock is always present (even when unchanged) because `Store::save()` writes unconditionally.

### 6. Drop v1.0 and v1.3 migration support

**Decision**: Delete `LockDataV1`, `LockDataV1_3`, `migrate_v1()`, `migrate_v1_3()`, `migrate_key()`, `derive_comment_from_v1_key()`.

**Rationale**: ~3 total users. The recovery path (delete `gx.lock`, run `gx tidy`) takes 5 seconds. Carrying 190+ lines of migration code for formats no one uses adds maintenance burden and test surface.

**What stays**: The flat format reader (`LockData` + `lock_from_data`) moves to `migration.rs` as a legacy reader. This handles the current flat-format lock files that exist in the wild (including this repo's own `.github/gx.lock`).

## Risks / Trade-offs

**[Risk] Full rewrite produces noisier git diffs on first migration** → One-time cost. After the first write in two-tier format, subsequent writes are deterministic and diffs are clean. Acceptable for a machine-generated file.

**[Risk] Callers forget to call `Store::save()` after mutation** → The API makes this hard to miss: there's only `load` and `save`. No implicit write-on-read. The compiler won't catch a missing `save()`, but the same is true of the current `apply_lock_diff` approach.

**[Risk] Breaking v1.0/v1.3 users** → Acceptable with ~3 users. The error message for unrecognized formats will be clear about the recovery path.

**[Trade-off] `plan()` now returns a `Lock` (heap allocation) alongside `LockDiff`** → The `Lock` is already computed inside `plan()` today — it's just not returned. No additional allocation; we're returning what was previously discarded.
