# Proposal: Use resolved version as workflow comment

## Why

This change simplifies the lock data model, removes a redundant concept, and changes the workflow annotation from specifier-derived (e.g., `# v4`) to resolved version (e.g., `# v4.2.1`). The workflow annotation change is user-visible but cosmetic — it improves the information shown in YAML comments without changing any functional behavior.

Delta specs are provided for the four affected main specs: `file-format`, `lock-reconciliation`, `domain-composition`, and `code-quality`.

## Problem

The lock's `Resolution` struct stores a `comment: VersionComment` field that is always derived from the manifest specifier (e.g., `^4` produces comment `"v4"`). This is redundant:

1. **The comment duplicates intent that already lives in the manifest.** The manifest says `^4` (policy); the comment says `v4` (same policy, different format). Two sources of truth for the same concept.
2. **The resolved version is more useful as a workflow annotation.** `# v4.2.1` tells you exactly what's pinned; `# v4` requires checking the lock file to know the actual version.
3. **`VersionComment` is a type with zero domain behavior** — it's a `String` newtype with only `as_str()` and `Display`. It exists solely to shuttle a derived string through the system.
4. **Two-phase comment population** — `lock_sync.rs` calls `lock.set()` then `lock.set_comment()` separately, with `is_complete()` validating comment consistency. This complexity exists only because the comment is stored rather than derived.

## Solution

1. **Remove `comment` from `Resolution`** — the struct becomes `Resolution { version: Version }`.
2. **Delete `VersionComment` type** — no longer needed anywhere.
3. **Use `resolution.version` as the workflow annotation** — all places that read `res.comment` now read `res.version`.
4. **Remove `comment` from lock TOML format** — no longer serialized. Old lock files with `comment` fields parse fine (serde ignores unknown fields; no `deny_unknown_fields`).
5. **Rename `Specifier::to_comment()` → `to_lookup_tag()`** — its only remaining caller is `ActionResolver::resolve()` which uses it to build the GitHub API lookup tag, not a workflow comment.
6. **Delete `Resolved::to_workflow_ref()`** — it produced `"SHA # specifier_comment"` which is the old format. Only used in tests.
7. **Delete `CrossRange::new_comment` field** from `Action` in `domain/action/upgrade.rs` — it stored the specifier-derived comment for the upgrade path. No longer needed since the resolved version is the comment.
8. **Remove `Lock::set_comment()`** and the `comment` parameter from `Lock::set()`.
9. **Simplify `Lock::is_complete()`** — remove the comment consistency check.

## Non-goals

- Changing the domain/infra serialization boundary (separate proposal).
- Renaming `Resolved` → `RegistryResolution` (separate proposal).
- Changing any CLI output, commands, or flags.

## Outcome

- `Resolution` is a single-field struct: `{ version: Version }`.
- `VersionComment` type deleted.
- `Lock::set()` takes 3 params instead of 4.
- `Lock::set_comment()` deleted.
- `is_complete()` simplified (no comment validation).
- Lock TOML no longer writes `comment` field.
- Workflow annotations show resolved version (e.g., `# v4.2.1` instead of `# v4`).
- `Specifier::to_comment()` renamed to `to_lookup_tag()`.
- `Resolved::to_workflow_ref()` deleted.
- `CrossRange::new_comment` deleted.

## Delta specs

- **file-format**: Remove `comment` from lock structure, all TOML examples, and migration scenarios. Add legacy comment drop scenario and workflow annotation requirement.
- **lock-reconciliation**: Remove comment match from `is_complete()` criteria. Remove "Stale comment" and "Only comment is stale" scenarios.
- **domain-composition**: Remove "Resolution comment updated to VersionComment" scenario. Add `Specifier::to_lookup_tag()` rename requirement.
- **code-quality**: Remove `VersionComment` newtype definition.

## Order

Apply before `serialization-boundary` and `rename-resolved`.
