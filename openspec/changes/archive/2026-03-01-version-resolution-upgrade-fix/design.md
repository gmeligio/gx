## Context

The upgrade system has a tag fabrication bug: `find_latest_upgrade` reformats the best candidate's semver components into a synthetic tag name (e.g., `v3.0.0-beta.2` → `v3`) that may not exist. The lock file also lacks the resolved version, making it impossible to detect same-SHA no-op upgrades.

## Goals / Non-Goals

**Goals:**
- Upgrade always produces actual tag names from the candidate list
- Lock entries carry the precise resolved version and semver specifier
- Upgrade detects same-SHA candidates via lock version comparison (no extra API calls)
- Single upgrade function, easy to test

**Non-Goals:**
- Changing the `--latest` flag name (deferred)
- Date-based upgrade validation
- Changing the manifest format

## Decisions

### D1: Single `find_upgrade_candidate` function

**Current design:** Two methods on `Version`:
- `find_upgrade()` — safe mode, constrains by precision, reformats output
- `find_latest_upgrade()` — latest mode, no constraint, reformats output

Both fabricate tags by reformatting semver components to match input precision.

**New design:** One free function (or method):

```rust
fn find_upgrade_candidate(
    manifest_version: &Version,    // range constraint source
    lock_version: Option<&Version>, // floor for "is this actually new?"
    candidates: &[Version],        // actual tag names from registry
    allow_major: bool,             // safe (false) vs latest (true)
) -> Option<Version>               // actual tag from candidates, never fabricated
```

Logic:
1. Parse `manifest_version` as semver → `manifest_semver`
2. Parse `lock_version` as semver (if present) → `lock_semver`
3. Compute floor = `max(manifest_semver, lock_semver)` (or just `manifest_semver` if no lock version)
4. For each candidate: parse as semver, skip if `<= floor`
5. If `!allow_major`: apply range constraint based on manifest precision:
   - Major (`v4`) or Minor (`v4.2`): `candidate.major == manifest_semver.major`
   - Patch (`v4.1.0`): `candidate.major == manifest_semver.major && candidate.minor == manifest_semver.minor`
6. Return the candidate tag (original string) with the highest parsed semver

**Why this is simpler:**
- One function instead of two
- No `VersionPrecision`-based reformatting
- Returns actual tags — impossible to fabricate
- Pre-releases handled naturally by semver crate comparison
- Lock version floor eliminates same-SHA upgrades without extra API calls

**Alternatives considered:**
- Patching the existing two functions to return actual tags — fixes fabrication but preserves unnecessary duplication.
- Adding SHA-based dedup in the apply phase — requires extra API calls during planning.

### D2: Version specifier semantics

The manifest version's precision determines the semver range specifier:

| Manifest | Precision | Specifier | Range |
|----------|-----------|-----------|-------|
| `v4` | Major | `^4` | >= 4.0.0, < 5.0.0 |
| `v4.2` | Minor | `^4.2` | >= 4.2.0, < 5.0.0 |
| `v4.1.0` | Patch | `~4.1.0` | >= 4.1.0, < 4.2.0 |

Major and Minor use caret (^) semantics. Patch uses tilde (~) semantics. This matches the existing `find_upgrade` filter logic — the filter was correct, only the output reformatting was wrong.

### D3: Lock file v1.3 — `version` and `specifier` fields

Add two new fields to lock entries:

```toml
version = "1.3"

[actions]
"actions/checkout@v6" = {
    sha = "de0fac2e...",
    version = "v6.2.3",
    specifier = "^6",
    repository = "actions/checkout",
    ref_type = "release",
    date = "2026-01-09T19:42:23Z"
}
```

- `version`: the most specific tag pointing to the same SHA as the resolved ref. Determined by SHA matching against the full tag list.
- `specifier`: the semver range derived from the manifest version's precision. A computed field for human readability and auditability.

Both fields are optional during deserialization for backward compatibility with v1.1 locks. When absent, the system operates as before (manifest version used as floor, no specifier displayed).

### D4: Resolving the precise version via SHA matching

After resolving a ref to a SHA, find the most specific version tag pointing to that SHA:

1. Fetch all version tags via `get_version_tags()` (already paginated)
2. For each tag, resolve to SHA via `fetch_ref()` or use cached data
3. Filter to tags whose SHA matches the resolved SHA
4. Among matches, pick the one with the highest semver (most specific)

This requires an additional API flow during tidy. Since `get_version_tags` is already called during upgrade, the data can be reused. For tidy, this is a new call per action.

**Trade-off:** More API calls during tidy. For the gx repo's ~9 actions, this adds ~9 paginated tag fetches plus tag resolution calls. Acceptable for correctness.

**Alternative considered:** Only populate `version` during upgrade (when tags are already fetched), leave it empty during tidy. Rejected because the lock should always be complete after tidy.

### D5: Upgrade uses lock version as floor

The upgrade comparison floor is `max(manifest_semver, lock_version_semver)`:

```
Manifest: "v4"        → 4.0.0
Lock:     "v4.2.1"    → 4.2.1
Floor:    max(4.0.0, 4.2.1) = 4.2.1

Candidate "v4.2.1":  4.2.1 <= 4.2.1  → SKIP (already locked)
Candidate "v4.3.0":  4.3.0 >  4.2.1  → VALID upgrade
```

When no lock version exists (new action, pre-1.3 lock), the floor falls back to `manifest_semver`. This is safe — it may produce a candidate that resolves to the same SHA, but that's a minor no-op rather than an error.

## Risks / Trade-offs

- **More API calls during tidy** — fetching the full tag list per action adds ~1 paginated call each. For typical repos this is well within rate limits.
- **Lock version absence in pre-1.3 locks** — graceful fallback to manifest-only comparison. First tidy run with new code populates the version field.
- **Retagged versions (adversarial edge case)** — if a maintainer retags `v4.2.1` to a different commit, the lock version comparison may miss it. `tidy` re-resolves and catches this naturally.
