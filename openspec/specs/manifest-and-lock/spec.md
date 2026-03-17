# Manifest and Lock

**User value:** Users' gx manifest and lock files are structured, consistent, and forward-compatible.

---

## File Format

### Requirement: Files have no format version field

Neither `gx.toml` nor `gx.lock` contains a format version field. Users never need to coordinate tool version with file version.

#### Scenario: Manifest has no gx section
- **GIVEN** a manifest written by any current gx version
- **THEN** the file contains `[actions]` and optionally `[actions.overrides]` and `[lint]`
- **AND** there is no `[gx]` section

#### Scenario: Lock has no version field
- **GIVEN** a lock file written by any current gx version
- **THEN** the file contains `[resolutions]` and `[actions]` sections
- **AND** there is no top-level `version` field

### Requirement: Forward-compatible reads

The parser SHALL ignore unknown TOML keys and sections without erroring. This applies to all sections in the lock file and unknown top-level sections in the manifest.

This requirement does NOT apply to `[lint.rules]` keys. Lint rule names are a closed set -- unrecognized rule names produce a parse error to catch typos early.

#### Scenario: Manifest with unknown top-level section
- **GIVEN** a manifest containing an unknown section `[metadata]`
- **WHEN** the manifest is parsed
- **THEN** parsing succeeds and the unknown section is ignored

#### Scenario: Lock entry with unknown field
- **GIVEN** a lock file entry containing an unknown field `priority = "high"`
- **WHEN** the lock file is parsed
- **THEN** parsing succeeds and the unknown field is ignored

#### Scenario: Unrecognized format produces an error
- **GIVEN** a lock file with non-TOML content or unrecognized TOML structure
- **WHEN** the lock file is loaded
- **THEN** an error is returned indicating the format is unrecognized

#### Scenario: Empty lock file returns a default empty lock
- **GIVEN** a lock file that exists but is empty
- **WHEN** the lock file is loaded
- **THEN** an empty lock is returned

---

## Write Behavior

### Requirement: Writes are deterministic and from scratch

Every write builds output fresh from the in-memory model. No prior file state carries between read and write. Unknown fields present in the input are not preserved in the output.

#### Scenario: Unknown fields are dropped on write
- **GIVEN** a lock file containing an unknown field `checksum = "abc123"` in an entry
- **WHEN** the lock is read and then written back
- **THEN** the unknown field is not present in the output

#### Scenario: Write output is deterministic
- **GIVEN** the same in-memory model
- **WHEN** written twice
- **THEN** both outputs are identical byte-for-byte

### Requirement: Entries are sorted lexicographically

Lock file entries are sorted by action ID then by specifier (in the resolutions section) or version (in the actions section). This keeps diffs minimal and review-friendly.

#### Scenario: Resolutions are sorted by action ID then specifier
- **GIVEN** resolutions for `actions/setup-node@^3` and `actions/checkout@^6`
- **WHEN** the lock is written
- **THEN** `actions/checkout` resolutions appear before `actions/setup-node` resolutions

#### Scenario: Actions are sorted by action ID then version
- **GIVEN** action entries for `actions/setup-node@v3.7.0` and `actions/checkout@v6.2.3`
- **WHEN** the lock is written
- **THEN** `actions/checkout` entries appear before `actions/setup-node` entries

---

## Manifest Structure

### Requirement: Manifest sections reflect user intent

The manifest contains `[actions]` mapping action IDs to version specifiers, with optional `[actions.overrides]` for per-workflow overrides. Empty sections are omitted.

#### Scenario: Manifest with actions and overrides
- **GIVEN** a manifest with `actions/checkout = "^4"` and an override `{ workflow = ".github/workflows/ci.yml", version = "^3" }`
- **WHEN** the manifest is written
- **THEN** the output contains both `[actions]` and `[actions.overrides]` sections

#### Scenario: Manifest with actions only
- **GIVEN** a manifest with `actions/checkout = "^4"` and no overrides
- **WHEN** the manifest is written
- **THEN** the output contains `[actions]` section only (no empty `[actions.overrides]`)

---

## Lock Structure

### Requirement: Lock file uses two-tier resolution and action sections

The lock file has two top-level sections:
- `[resolutions]` maps each (action, specifier) pair to a resolved version.
- `[actions]` maps each (action, version) pair to commit metadata (sha, repository, ref_type, date).

This two-tier design means multiple specifiers can share a single action entry, avoiding duplication.

#### Scenario: Standard lock entry
- **GIVEN** an action `actions/checkout@^6` resolved to version `v6.2.3` with SHA `de0fac2e...`
- **THEN** the lock file contains a resolution mapping `("actions/checkout", "^6")` to version `v6.2.3`, and an action entry mapping `("actions/checkout", "v6.2.3")` to its commit metadata

#### Scenario: Multiple specifiers share one action entry
- **GIVEN** actions `actions/checkout@^4` and `actions/checkout@^4.2` both resolve to `v4.2.1`
- **THEN** the lock file has two resolution entries pointing to one action entry for `v4.2.1`

#### Scenario: Subpath action stores base repository
- **GIVEN** an action `github/codeql-action/upload-sarif@^3` resolved against repository `github/codeql-action`
- **THEN** the action entry's `repository` field is `github/codeql-action`

#### Scenario: Non-semver branch ref
- **GIVEN** an action `actions/checkout@main` resolved to SHA `abc123...`
- **THEN** the resolution uses `main` as both the specifier key and the version value
- **AND** the action entry has `ref_type = "branch"`

### Requirement: Roundtrip integrity for known fields

Reading and writing a lock file with no logical changes produces byte-for-byte identical output.

#### Scenario: Two-tier roundtrip
- **GIVEN** a two-tier lock file with resolutions and action entries
- **WHEN** the lock is read and then written back
- **THEN** the output is byte-for-byte identical to the input

---

## Lock Completeness

### Requirement: A spec is complete only when both tiers are fully populated

A spec is complete when its resolution entry has a non-empty version AND the corresponding action entry has all metadata fields populated (sha, repository, ref_type, date). Incomplete specs are detected and repaired automatically.

#### Scenario: Complete resolution and action entry
- **GIVEN** a resolution for `(actions/checkout, ^4)` with `version = "v4.2.1"`
- **AND** an action entry for `(actions/checkout, v4.2.1)` with all metadata populated
- **THEN** the spec is complete and no work is performed

#### Scenario: Missing resolution triggers full resolution
- **GIVEN** no resolution exists for `(actions/checkout, ^4)`
- **WHEN** tidy runs
- **THEN** the system resolves the version from the network and creates both resolution and action entries

#### Scenario: Resolution exists but action entry is missing
- **GIVEN** a resolution for `(actions/checkout, ^4)` pointing to `v4.2.1`
- **AND** no action entry for `(actions/checkout, v4.2.1)`
- **WHEN** tidy runs
- **THEN** the system re-resolves from scratch because the previously resolved version may no longer be available

#### Scenario: Action entry missing metadata triggers targeted repair
- **GIVEN** a resolution pointing to a version whose action entry is missing `ref_type`
- **WHEN** tidy runs
- **THEN** the system fetches only the missing metadata without re-resolving the version

#### Scenario: Non-semver spec with complete data
- **GIVEN** a resolution for `(actions/checkout, main)` with `version = "main"` and a complete action entry
- **THEN** the spec is complete

### Requirement: Self-healing on schema additions

When new fields are added to the action entry schema, existing lock files are repaired automatically. The completeness check inspects all required fields, so missing fields are detected and filled on the next tidy run without any migration code.

---

## Tidy and Orphan Cleanup

### Requirement: Tidy removes unreferenced action entries

After resolving and retaining entries, tidy removes action entries not referenced by any resolution. This keeps the lock file free of stale data.

#### Scenario: Specifier change leaves orphaned action entry
- **GIVEN** a resolution `(actions/checkout, ^4)` pointing to `v4.2.1`
- **WHEN** the user changes the manifest specifier to `^4.3` and tidy resolves to `v4.3.0`
- **THEN** the old `v4.2.1` action entry is removed

#### Scenario: Shared action entry is not orphaned
- **GIVEN** resolutions `(actions/checkout, ^4)` and `(actions/checkout, ^4.2)` both pointing to `v4.2.1`
- **WHEN** the `^4.2` resolution is removed
- **THEN** the `v4.2.1` action entry is NOT removed because `^4` still references it

#### Scenario: Upgrade orphans old action entry
- **GIVEN** a resolution `(actions/checkout, ^4)` pointing to `v4.2.1`
- **WHEN** `gx upgrade` resolves `^4` to `v4.3.0`
- **THEN** the old `v4.2.1` action entry is removed

#### Scenario: Branch ref re-resolution updates in place
- **GIVEN** a resolution `(actions/checkout, main)` pointing to version `main` with SHA `aaa...`
- **WHEN** tidy re-resolves `main` and gets SHA `bbb...`
- **THEN** the action entry is updated with the new SHA
- **AND** no orphaned entry is created because the key (`main`) does not change

---

## Format Migration

### Requirement: Migration happens transparently on write

Migration occurs when a write command (tidy, init, upgrade) outputs files. Read-only commands do not modify files. The lock file is always written in the current two-tier format via full rewrite -- no migration flags, no migration messages for lock files.

#### Scenario: Flat lock is silently migrated on tidy
- **GIVEN** a flat-format lock file (single `[actions]` section with `"action@specifier"` composite keys)
- **WHEN** `gx tidy` runs
- **THEN** the lock file is rewritten in two-tier format
- **AND** no "migrated" message is printed

#### Scenario: Flat entries deduplicating to one action entry
- **GIVEN** a flat lock file with two entries for `actions/checkout@^4` and `actions/checkout@^4.2` both at version `v4.2.1`
- **WHEN** migration runs
- **THEN** the output has two resolution entries and one action entry

#### Scenario: Flat entry with missing version uses specifier as fallback
- **GIVEN** a flat lock entry with `version` absent or empty
- **WHEN** migration runs
- **THEN** the resolution is created with the specifier as the version fallback
- **AND** the entry is detected as incomplete on next tidy run

#### Scenario: Legacy comment fields are dropped
- **GIVEN** a lock file with `comment` fields in resolution entries
- **WHEN** a write command runs
- **THEN** the `comment` fields are dropped silently

#### Scenario: Two-tier lock with no changes is rewritten identically
- **GIVEN** a two-tier lock file with no logical changes
- **WHEN** a write command runs
- **THEN** the lock file content is byte-for-byte identical after the write

### Requirement: Manifest v1 migration reports to user

v1 manifests (with `"v4"` style values instead of semver specifiers) are transparently converted on write. Unlike lock migration, manifest migration reports to the user.

#### Scenario: v1 manifest migration message
- **GIVEN** a v1 manifest
- **WHEN** a write command runs
- **THEN** the output includes `migrated gx.toml -> semver specifiers`

---

## Workflow Annotation

### Requirement: Workflow comments show the resolved version

When gx writes a pinned action reference to a workflow file, the inline YAML comment shows the resolved version from the lock, not a specifier-derived string.

#### Scenario: Version annotation uses resolved version
- **GIVEN** a manifest specifier `^4` resolved to version `v4.2.1` with SHA `abc123...`
- **WHEN** gx writes the workflow file
- **THEN** the output is `uses: actions/checkout@abc123... # v4.2.1`

#### Scenario: Bare SHA specifier has no annotation
- **GIVEN** a manifest specifier that is a bare SHA
- **WHEN** gx writes the workflow file
- **THEN** the output is `uses: actions/checkout@abc123...` with no inline comment

---

## Architectural Guardrail

### Guardrail: TOML generation uses a document builder, not string formatting

All TOML output for `gx.toml` and `gx.lock` SHALL be produced by a TOML document builder library, not manual string formatting. This prevents subtle serialization bugs (quoting, escaping, table ordering) that would corrupt user files.
