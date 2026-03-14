## Context

gx manages two TOML files: `gx.toml` (human-authored manifest) and `gx.lock` (machine-managed lock). Both currently embed format version numbers and use manual `format!()`/`writeln!()` string building for writes. The lock file uses inline tables, making diffs noisy. The `toml_edit` crate is already a dependency, used only for surgical lock diffs (`apply_lock_diff`).

## Goals / Non-Goals

**Goals:**

- Remove version fields from both file formats
- Use `toml_edit` for all writes (no `format!()` string building)
- Switch lock entries from inline tables to standard TOML tables
- Keep reading old formats with migration
- Simplest possible architecture

**Non-Goals:**

- Round-trip preservation of unknown fields or comments
- Changing the domain model (`Manifest`, `Lock`, `Entry`, `LockKey`)
- Changing which fields exist in either format (structural change only)

## Decisions

### Decision 1: Fresh build on write — no document round-tripping

Read and write are fully decoupled. No `DocumentMut` is carried between them.

```
Read:  file → serde deserialize → domain types (unknowns ignored)
Write: domain types → build fresh DocumentMut → file
```

`Parsed<T>` stays as-is: `{ value, migrated }`. No `toml_edit` types leak into domain or parsed results.

Forward compatibility is read-only: old gx ignores unknown fields. If old gx writes the file, unknown fields are dropped — new gx treats them as optional and regenerates. This is how every package manager works.

### Decision 2: Lock entries as standard TOML tables

```toml
[actions."actions/checkout@^6"]
sha = "de0fac2e4500dabe0009e67214ff5f5447ce83dd"
version = "v6.0.2"
comment = "v6"
repository = "actions/checkout"
ref_type = "tag"
date = "2026-01-09T19:42:23Z"
```

Better git diffs, more readable. Field order is fixed: sha, version, comment, repository, ref_type, date.

### Decision 3: Manifest overrides stay as inline tables

```toml
[actions.overrides]
"actions/checkout" = [
  { workflow = ".github/workflows/ci.yml", version = "^3" },
]
```

Small records (2–4 fields), human-authored. Inline is more scannable than `[[array.of.tables]]`.

### Decision 4: No version fields

Neither file contains a format version. The contract: new fields are always optional, readers ignore unknowns.

Old formats are detected structurally:
- **Lock has `version` field** → old format, migrate
- **Lock has no `version` field** → new format
- **Manifest has `[gx]` section** → old format, strip it, migrate
- **Manifest has no `[gx]` section** → current format

### Decision 5: Unify lock write paths

Currently two write paths: `save()` (full `format!()` rewrite) and `apply_lock_diff()` (surgical `toml_edit` patch). Both become `toml_edit`-based fresh builds. `apply_lock_diff` builds a fresh document, applies the diff, writes it. `save()` builds from the full domain model.

### Decision 6: `toml_edit` for manifest writes too

Replace `format_manifest_toml()` string building with `toml_edit::DocumentMut` construction. Same fresh-build pattern as the lock.

## Risks / Trade-offs

**[Breaking change]** → Older gx binaries will fail on files without version fields. This is a semver-major change.

**[Comments dropped on write]** → gx.toml comments are lost when gx rewrites the file (during `tidy` or `create`). Acceptable: gx writes are infrequent and produce canonical output.

**[No structural escape hatch]** → Without a version field, a future structural change has no dispatch mechanism. Mitigation: the data model is stable; if needed, a new top-level key can serve as a feature flag.
