# Proposal: Rich lock entry metadata

## Problem

The `gx.lock` file currently stores only the commit SHA per action entry. There is no record of *when* the commit was made, *what kind of reference* the user specified, or *which repository* was resolved. This makes it hard to audit lock entries, understand their provenance, or build features that depend on temporal or contextual metadata (e.g., staleness detection, upgrade advisories).

## Proposed change

Enrich each lock entry from a plain string (`sha`) to a structured inline table with four fields:

| Field | Type | Description |
|-------|------|-------------|
| `sha` | string | The resolved commit SHA (40 hex chars) |
| `repository` | string | The GitHub repository that was queried (e.g., `github/codeql-action` for subpath actions) |
| `ref_type` | enum | What the ref resolved to: `release`, `tag`, `branch`, or `commit` |
| `date` | string | RFC 3339 timestamp whose meaning depends on `ref_type` |

### Date semantics by ref_type

| ref_type | Date source | GitHub API field |
|----------|-------------|-----------------|
| `release` | When the release was published | `Release.published_at` |
| `tag` | When the tag was created (annotated) or when the commit was made (lightweight) | `Tag.tagger.date` or `Commit.committer.date` |
| `branch` | When the commit was made | `Commit.committer.date` |
| `commit` | When the commit was made | `Commit.committer.date` |

### Lock file format change

Before (v1.0):

```toml
version = "1.0"

[actions]
"actions/checkout@v6" = "de0fac2e4500dabe0009e67214ff5f5447ce83dd"
```

After (v2.0):

```toml
version = "2.0"

[actions]
"actions/checkout@v6" = { sha = "de0fac2e...", repository = "actions/checkout", ref_type = "release", date = "2026-02-15T10:35:00Z" }
"some/action@main" = { sha = "def456...", repository = "some/action", ref_type = "branch", date = "2026-01-10T09:00:00Z" }
```

## Scope

- Bump lock file version from `"1.0"` to `"2.0"`.
- Change the `VersionRegistry` trait and `ResolvedAction` to carry richer resolution data (repository, ref_type, date) — Option 2 from exploration.
- Extend `GithubRegistry` to fetch the best available date during resolution: try release `published_at`, then annotated tag `tagger.date`, then commit `committer.date`.
- Migrate existing v1.0 lock files by fetching metadata from GitHub for each entry.
- Update serialization/deserialization (`LockData`) from `HashMap<String, String>` to `HashMap<String, ActionEntry>`.

## Non-goals

- Adding the commit URL to the lock file (derivable from `repository` + `sha`).
- Changing the manifest format.
- Changing CLI output or display format (can be done in a follow-up).

## Risks

- **Extra API calls during resolution**: fetching release/tag/commit dates adds 1-2 HTTP requests per action. May need to consider rate limits for repos with many actions.
- **Migration requires GitHub token**: v1.0 → v2.0 migration needs to fetch metadata from GitHub, which requires `GITHUB_TOKEN`. If unavailable, migration must degrade gracefully.
