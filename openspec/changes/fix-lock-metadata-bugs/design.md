## Context

The lock entry metadata feature added `ref_type`, `date`, and `repository` to lock entries. The implementation has three bugs:

1. **`validate_and_correct` bypasses metadata resolution.** When workflows have SHA-pinned actions (the normal state after `gx tidy`), the code path calls `tags_for_sha` for version checking but then hardcodes `RefType::Commit` and empty date in all four return paths. Since this is the primary path during tidy, all lock entries get wrong metadata.

2. **`CommitDetailResponse` parses the wrong JSON level.** The GitHub `/commits/{sha}` API returns `commit.committer.date` (nested), but the struct deserializes the top-level `committer` field, which is a GitHub user object with no `date`.

3. **`resolve_ref` never returns `RefType::Release`.** After resolving a tag, it doesn't check for an associated GitHub Release, so the Release arm in `lookup_sha` is dead code.

## Goals / Non-Goals

**Goals:**
- All lock entries always have correct `ref_type` and `date`, regardless of which tidy code path runs
- Single code path for metadata resolution (eliminate the dual-path bug class)
- Easy to test: each function does one thing

**Non-Goals:**
- Changing the lock file format or version
- Changing the `VersionRegistry` trait signature
- Optimizing API call count

## Decisions

### D1: Replace `validate_and_correct` with `correct_version`

**Current design:** `ActionResolver` has two methods that produce `ResolvedAction`:
- `resolve()` → calls `lookup_sha`, returns full metadata
- `validate_and_correct()` → calls `tags_for_sha`, hardcodes metadata (the bug)

**New design:** `ActionResolver` has two orthogonal methods:
- `resolve(spec)` → calls `lookup_sha`, returns full metadata (unchanged)
- `correct_version(id, sha)` → calls `tags_for_sha`, returns `(Version, bool)` — just a version, no metadata

`resolve()` is the **only** way to produce a lock entry. `correct_version()` is a pure version-correction step used before `resolve()` when workflows already have SHAs.

```
BEFORE:
  validate_and_correct(spec, sha) → ResolutionResult with hardcoded metadata
  resolve(spec) → ResolutionResult with real metadata

AFTER:
  correct_version(id, sha) → (Version, was_corrected)
  resolve(spec) → ResolutionResult with real metadata  (ONLY metadata path)
```

**Alternatives considered:**
- Patching `validate_and_correct` to call `lookup_sha` internally — fixes the bug but preserves the confusing dual-path design.
- Adding a `lookup_metadata(id, version, sha)` method to the trait — adds API surface without simplifying.

### D2: Tidy orchestrates version correction + resolution

`update_lock` in `tidy.rs` becomes:

```
for spec in specs:
    if workflow has SHA:
        (version, corrected) = resolver.correct_version(id, sha)
        resolve_spec = ActionSpec(id, version)
        resolved = resolver.resolve(resolve_spec)
        resolved = resolved.with_sha(workflow_sha)   // keep pinned SHA
        lock.set(resolved)
        if corrected: record correction, update manifest
    else if not in lock:
        resolved = resolver.resolve(spec)
        lock.set(resolved)
```

The SHA override (`with_sha`) is necessary because `resolve()` returns the SHA that the tag currently points to, which may differ from the workflow's pinned SHA if the tag was moved. We trust the workflow's SHA.

### D3: Add `ResolvedAction::with_sha()`

A builder method that returns a new `ResolvedAction` with the SHA replaced. Keeps `ResolvedAction` fields immutable (no `pub mut`), makes the override explicit.

### D4: Fix `CommitDetailResponse` nesting

The GitHub commits API response structure:

```json
{
  "commit": {
    "committer": { "date": "2026-01-09T..." }
  },
  "committer": { "login": "web-flow" }
}
```

Fix: add a `CommitObject` wrapper struct:

```rust
struct CommitDetailResponse {
    commit: CommitObject,
}
struct CommitObject {
    committer: Option<CommitterInfo>,
}
```

### D5: Add Release detection in `resolve_ref`

After tag resolution succeeds in `resolve_ref`, check for a GitHub Release before returning:

```
resolve_ref(owner_repo, ref_name):
  ...
  tag found → sha
    fetch_release_date(base_repo, ref_name) succeeds? → (sha, Release)
    no release? → (sha, Tag)
  ...
```

This makes the `RefType::Release` arm in `lookup_sha` live. The existing date priority logic in `lookup_sha` already handles Release correctly (uses `published_at`).

**Trade-off:** One extra API call per tag resolution to check for releases. Acceptable since resolution already makes 2-4 calls per action.

## Risks / Trade-offs

- **Extra API call per action during tidy** — `validate_and_correct` previously made 1 call (`tags_for_sha`). The new flow makes 1 + 2-4 calls (correction + resolution). For the gx repo's 9 actions this is ~30-40 calls, well within the 5000/hour limit. → Acceptable.
- **SHA divergence** — If a tag was moved, `resolve()` returns the new SHA but we override with the workflow's old SHA. The metadata (ref_type, date) comes from the current tag, not the pinned commit. → This is correct: ref_type describes the version reference, and date describes when that version was published. Both are version-level, not commit-level.
