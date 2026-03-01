# Design: Rich lock entry metadata

## Architecture overview

```
                  ┌─────────────────────────────────────────────────┐
                  │              DOMAIN LAYER                       │
                  │                                                 │
                  │  ┌──────────┐    ┌──────────────┐               │
                  │  │ RefType  │    │  LockEntry   │               │
                  │  │ (enum)   │    │ {sha,repo,   │               │
                  │  │          │    │  ref_type,   │               │
                  │  │ Release  │    │  date}       │               │
                  │  │ Tag      │◄───┤              │               │
                  │  │ Branch   │    └──────┬───────┘               │
                  │  │ Commit   │           │                       │
                  │  └──────────┘           │ stored in             │
                  │                         ▼                       │
                  │  ┌────────────────────────────────────┐         │
                  │  │  Lock                              │         │
                  │  │  HashMap<LockKey, LockEntry>       │         │
                  │  │  (was HashMap<LockKey, CommitSha>) │         │
                  │  └────────────────────────────────────┘         │
                  │                                                 │
                  │  ┌──────────────────────────────────────────┐   │
                  │  │  VersionRegistry trait                   │   │
                  │  │  lookup_sha() → ResolvedRef              │   │
                  │  │  (was → CommitSha)                       │   │
                  │  └──────────────────────────────────────────┘   │
                  └─────────────────────────────────────────────────┘
                                        │
                                        ▼
                  ┌─────────────────────────────────────────────────┐
                  │           INFRASTRUCTURE LAYER                  │
                  │                                                 │
                  │  ┌──────────────────────────────────────────┐   │
                  │  │  GithubRegistry                          │   │
                  │  │  resolve_ref() → determines ref_type     │   │
                  │  │  fetch_date()  → best available date     │   │
                  │  │  returns ResolvedRef{sha,repo,type,date} │   │
                  │  └──────────────────────────────────────────┘   │
                  │                                                 │
                  │  ┌──────────────────────────────────────────┐   │
                  │  │  FileLock (serialization)                │   │
                  │  │  LockData → HashMap<String, ActionEntry> │   │
                  │  │  ActionEntry{sha,repository,ref_type,    │   │
                  │  │             date}                        │   │
                  │  │  version = "1.1"                         │   │
                  │  └──────────────────────────────────────────┘   │
                  └─────────────────────────────────────────────────┘
```

## Key decisions

### D1: New domain types

**`RefType` enum** in `domain/action.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefType {
    Release,  // tag with a GitHub Release → date from published_at
    Tag,      // tag without a Release → date from tagger.date or committer.date
    Branch,   // branch ref → date from committer.date
    Commit,   // direct SHA → date from committer.date
}
```

Serialized as lowercase strings: `"release"`, `"tag"`, `"branch"`, `"commit"`.

**`LockEntry` struct** in `domain/lock.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockEntry {
    pub sha: CommitSha,
    pub repository: String,
    pub ref_type: RefType,
    pub date: String, // RFC 3339
}
```

This replaces bare `CommitSha` in the Lock's internal HashMap.

### D2: VersionRegistry trait change

The `lookup_sha` method changes its return type:

```rust
// Before
fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<CommitSha, ResolutionError>;

// After
fn lookup_sha(&self, id: &ActionId, version: &Version) -> Result<ResolvedRef, ResolutionError>;
```

Where `ResolvedRef` is a new domain struct:

```rust
pub struct ResolvedRef {
    pub sha: CommitSha,
    pub repository: String,
    pub ref_type: RefType,
    pub date: String,
}
```

This is a domain type (not infrastructure) because `RefType` and the date are domain concepts the lock needs to store.

### D3: GithubRegistry resolution with ref_type detection

The current `resolve_ref()` tries three paths in order. The new version tracks which path succeeded:

```
resolve_ref(owner_repo, ref_name)
    │
    ├─ Is 40-char hex? → ref_type = Commit
    │   └─ GET /repos/{repo}/commits/{sha} → committer.date
    │
    ├─ Try /git/ref/tags/{ref_name}
    │   └─ Found? → check for Release
    │       ├─ GET /repos/{repo}/releases/tags/{ref_name}
    │       │   └─ Found? → ref_type = Release, date = published_at
    │       │
    │       └─ No release → check tag type
    │           ├─ object.type == "tag" (annotated)
    │           │   └─ GET /git/tags/{sha} → ref_type = Tag, date = tagger.date
    │           │
    │           └─ object.type == "commit" (lightweight)
    │               └─ GET /commits/{sha} → ref_type = Tag, date = committer.date
    │
    ├─ Try /git/ref/heads/{ref_name}
    │   └─ Found? → ref_type = Branch
    │       └─ GET /commits/{sha} → committer.date
    │
    └─ Try /commits/{ref_name}
        └─ Found? → ref_type = Commit
            └─ committer.date from same response
```

**Date field names match GitHub API** (used internally in the Rust structs):
- `published_at` for releases
- `tagger.date` (accessed as `tagger_date` in Rust) for annotated tags
- `committer.date` (accessed as `committer_date` in Rust) for commits

All are stored as a single `date` string in `LockEntry` — the `ref_type` tells you the provenance.

### D4: Repository field derivation

The `repository` field is the GitHub repository that was actually queried. For simple actions like `actions/checkout`, it's the same as the action ID. For subpath actions like `github/codeql-action/upload-sarif`, the repository is `github/codeql-action`.

`ActionId::base_repo()` already computes this. The `GithubRegistry` uses the same logic internally. The resolved repository will be returned from the registry as part of `ResolvedRef` for consistency — the registry records what it actually queried.

### D5: Lock file format migration (v1.0 → v1.1)

The existing migration mechanism in `parse_lock()` detects `version != LOCK_FILE_VERSION` and rewrites. For v1.1, migration must:

1. Parse the old format (detect that values are plain strings, not tables)
2. For each entry, fetch metadata from GitHub (repository, ref_type, date)
3. Write the new format with `version = "1.1"`

**Graceful degradation**: If `GITHUB_TOKEN` is unavailable during migration, the migration should warn and populate entries with defaults:
- `repository` = derived from `ActionId::base_repo()`
- `ref_type` = `Tag` (most common case for pinned actions)
- `date` = empty string `""`

This avoids blocking users who don't have a token set up. The metadata will be filled in correctly on the next `gx tidy` or `gx upgrade` that resolves the action.

### D6: Impact on Lock domain operations

Methods that change:

| Method | Before | After |
|--------|--------|-------|
| `Lock::new()` | `HashMap<LockKey, CommitSha>` | `HashMap<LockKey, LockEntry>` |
| `Lock::get()` | `Option<&CommitSha>` | `Option<&LockEntry>` |
| `Lock::set()` | takes `&ResolvedAction` → stores `CommitSha` | takes richer input → stores `LockEntry` |
| `Lock::entries()` | `Iterator<(&LockKey, &CommitSha)>` | `Iterator<(&LockKey, &LockEntry)>` |
| `Lock::build_update_map()` | reads SHA from `CommitSha` | reads SHA from `LockEntry.sha` |

Callers that only need the SHA (e.g., `build_file_update_map` in tidy.rs) access it via `entry.sha`.

### D7: ResolvedAction changes

`ResolvedAction` gains the new fields so it can carry the full resolution result into `Lock::set()`:

```rust
pub struct ResolvedAction {
    pub id: ActionId,
    pub version: Version,
    pub sha: CommitSha,
    pub repository: String,   // new
    pub ref_type: RefType,     // new
    pub date: String,          // new
}
```

This means `ActionResolver::resolve()` and `validate_and_correct()` must thread the metadata through their return paths.

### D8: Inline table serialization

The lock file SHALL serialize action entries as TOML inline tables under a single `[actions]` header — not as separate `[actions."key"]` table headers. This matches the style used in `gx.toml` where each action is a single key = value line.

**Why not `toml::to_string_pretty`?**

The `toml` 0.9 crate's `to_string_pretty` always produces expanded tables (`[actions."key"]\nfield = value\n...`). There is no API to request inline table output. The `toml_writer` low-level crate underneath doesn't expose inline table formatting either.

**Approach: manual string building**

The lock format is simple enough (version + flat map of key → 4-field struct) that manual serialization is the cleanest solution:

```rust
fn serialize_lock(lock: &Lock) -> String {
    let mut out = format!("version = \"{LOCK_FILE_VERSION}\"\n\n[actions]\n");
    let mut entries: Vec<_> = lock.entries().collect();
    entries.sort_by_key(|(k, _)| k.to_string());
    for (key, entry) in entries {
        writeln!(out, "\"{key}\" = {{ sha = \"{}\", repository = \"{}\", ref_type = \"{}\", date = \"{}\" }}",
            entry.sha, entry.repository, entry.ref_type, entry.date).unwrap();
    }
    out
}
```

- No new dependencies
- Deterministic sorted output
- Deserialization is unchanged — TOML parses inline and expanded tables identically
- Round-trip: write inline → read via `toml::from_str` → write inline (stable)

**Backward compatibility**: Both the old expanded format and the new inline format deserialize to the same `LockData` struct. The lock file version stays at `"1.1"` — the new fields are additive and older parsers that don't recognize them will simply ignore the inline table structure. No version bump needed.

### D9: Extra API calls and rate limits

For each action during resolution, the new flow may make up to 3 additional API calls (release check, tag object fetch, commit date fetch) beyond the current 1-3 calls for SHA resolution. For repositories with many actions, this could approach GitHub's rate limit (5000/hour for authenticated requests).

Mitigation: The commit date can often be fetched from the same response that resolves the SHA (the `/commits/{ref}` endpoint returns both). For the tag → release check, it's one additional call. Annotated tag metadata requires one more. In practice, most actions resolve via tags, so it's typically +1-2 calls per action.
