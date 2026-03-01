## Context

The lock update logic in `tidy.rs::update_lock` uses `lock.has(&key)` to decide whether to process an entry. If the entry exists, it's skipped entirely — even if fields are missing. This worked when all fields were set at creation time, but broke when `specifier` and `version` were added to the schema: pre-existing entries never got these new fields populated.

The root cause is structural. `ResolvedAction` carries fields from three different sources:
- **Resolution** (network): sha, repository, ref_type, date — from `lookup_sha()`
- **Refinement** (network): resolved_version — from `tags_for_sha()`
- **Derivation** (local): specifier — from `Version::specifier()`

These are tangled in `populate_resolved_fields()`, which must be called after `resolve()` as a post-processing step. Because it's a single function doing two network calls plus a local computation, there's no way to run just the cheap local part for entries that only need a specifier.

Additionally, SHA correction (`correct_version`) and version refinement both call `tags_for_sha` — the same underlying operation applied to different targets (manifest vs lock).

## Goals / Non-Goals

**Goals:**
- Lock entries self-report completeness via `is_complete()`
- Incomplete entries trigger only the operations they need (no unnecessary network calls)
- `ResolvedAction` carries only resolution output — no derived or refined fields
- Three distinct operations (RESOLVE, REFINE, DERIVE) with clear boundaries
- Future lock schema additions self-heal on next tidy run without migration code

**Non-Goals:**
- Changing the lock file format or version — the fields stay the same
- Changing the lock file serialization — `infrastructure/lock.rs` is unchanged
- Optimizing network calls beyond avoiding unnecessary ones
- Changing how the manifest or workflows are synced

## Decisions

### 1. Remove `resolved_version` and `specifier` from `ResolvedAction`

`ResolvedAction` becomes the pure output of RESOLVE: `(sha, repository, ref_type, date)`. It no longer accumulates fields from other operations.

**Alternative**: Keep `ResolvedAction` as-is and fix the call sites. Rejected because it perpetuates the conflation — the struct would still imply these fields come from resolution.

### 2. Eliminate `populate_resolved_fields`

Replace with two separate operations called independently:
- **REFINE**: `tags_for_sha(action, sha) → Version` — finds the most specific version tag for a SHA. Used for both lock `version` field and manifest SHA correction.
- **DERIVE**: `Version::specifier() → Option<String>` — already exists, just needs to be called at the right time.

These are called based on what the lock entry is missing, not bundled into a single post-processing step.

**Alternative**: Split `populate_resolved_fields` into two functions but keep them coupled. Rejected because the whole point is that they're independent operations with different cost profiles (network vs local).

### 3. `LockEntry::is_complete()` as the single completeness check

The lock entry validates itself. The tidy flow asks "is this entry complete?" instead of "does this entry exist?". The method checks all required fields: `version`, `specifier`, `repository`, `date`.

The `specifier` check needs the manifest version to validate correctness (not just presence). For example, if the manifest version changed from `v6` to `v6.1`, the specifier should change from `^6` to `^6.1`. So the signature is `is_complete(&self, manifest_version: &Version) -> bool`.

### 4. Two-phase tidy flow

Phase 1 — **Manifest correctness**: Sync manifest with workflows, correct versions using REFINE where workflow SHAs provide truth.

Phase 2 — **Lock completeness**: For each manifest spec, check `is_complete()`. For incomplete entries, run only the needed operations:
- No entry → RESOLVE + REFINE + DERIVE
- Missing `version` → REFINE (network)
- Missing/wrong `specifier` → DERIVE (local, no network)
- Missing `date`/`repository` → RESOLVE (network)

This replaces the current interleaved logic in `update_lock` where SHA correction and lock population are mixed.

### 5. SHA correction uses REFINE, not a separate `correct_version`

`correct_version` currently wraps `tags_for_sha` with version selection logic (`select_best_tag`). This same logic is needed for the lock `version` field. Unify them: REFINE is `tags_for_sha` + `select_best_tag`, used by both Phase 1 (manifest correction) and Phase 2 (lock version population).

The `ActionResolver::correct_version` method can be kept but reframed as "refine version for SHA" — same implementation, clearer name.

## Risks / Trade-offs

**Risk**: Entries with `specifier: Some("")` (from TOML parsing of `specifier = ""`) need to be treated as missing, not present.
→ Mitigation: `is_complete()` checks for both `None` and empty string.

**Risk**: REFINE requires a network call. Entries missing only `version` (but having sha, specifier, etc.) will trigger a network call that wasn't previously needed.
→ Mitigation: This only happens for entries that are genuinely incomplete. In practice, entries from before `version` was added are the only ones affected, and they need the data anyway.

**Risk**: Changing `ResolvedAction` is a breaking change to an internal struct used across many call sites.
→ Mitigation: The struct is internal to gx-lib. All callers are within the crate and can be updated together.
