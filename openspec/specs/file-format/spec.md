## Versionless Format

### Requirement: Versionless file formats
Neither `gx.toml` nor `gx.lock` SHALL contain a format version field.

#### Scenario: New manifest has no gx section
- **GIVEN** a manifest written by the current gx version
- **THEN** the file contains `[actions]` and optionally `[actions.overrides]` and `[lint]`
- **AND** there is no `[gx]` section

#### Scenario: New lock has no version field
- **GIVEN** a lock file written by the current gx version
- **THEN** the file contains `[resolutions]` and `[actions]` sections with standard TOML tables
- **AND** there is no top-level `version` field

### Requirement: Forward-compatible reads
The parser SHALL ignore unknown TOML keys and sections without erroring. This applies to `[resolutions]` and `[actions]` sections in the lock file, and unknown top-level sections in the manifest.

This requirement does NOT apply to `[lint.rules]` keys in the manifest. Lint rule names are a closed set deserialized as a `RuleName` enum — unrecognized rule names produce a parse error (see lint spec). This distinction is intentional: unknown data fields should be forward-compatible, but misconfigured lint rules should fail early to catch typos.

#### Scenario: Manifest with unknown top-level section
- **GIVEN** a manifest containing an unknown section `[metadata]`
- **WHEN** the manifest is parsed
- **THEN** parsing succeeds and the unknown section is ignored

#### Scenario: Resolution entry with unknown field
- **GIVEN** a resolution entry containing an unknown field `priority = "high"`
- **WHEN** the lock file is parsed
- **THEN** parsing succeeds and the unknown field is ignored

#### Scenario: Action entry with unknown field
- **GIVEN** an action entry containing an unknown field `checksum = "abc123"`
- **WHEN** the lock file is parsed
- **THEN** parsing succeeds and the unknown field is ignored

---

## Write Format

### Requirement: Writes use toml_edit
All file writes for `gx.toml` and `gx.lock` SHALL use `toml_edit::DocumentMut` to build the output. No manual string formatting (`format!()`, `writeln!()`) SHALL be used for TOML generation.

### Requirement: Fresh build on write
Writes SHALL build a fresh `DocumentMut` from the domain model. No prior document state is carried between read and write.

#### Scenario: Unknown fields are not preserved on write
- **GIVEN** a lock file containing an unknown field `checksum = "abc123"` in an entry
- **WHEN** the lock is read and then written back
- **THEN** the unknown field is not present in the output

#### Scenario: Write output is deterministic
- **GIVEN** the same domain model
- **WHEN** written twice
- **THEN** both outputs are identical byte-for-byte

---

## Manifest Format

### Requirement: Manifest structure
The manifest SHALL NOT contain a `[gx]` section. The file format is `[actions]` with optional `[actions.overrides]` and `[lint]` sections.

### Requirement: Manifest writes use toml_edit
Overrides SHALL be written as inline tables within arrays. Empty `[actions.overrides]` section is not written if no overrides exist.

#### Scenario: Manifest with actions and overrides
- **GIVEN** a manifest with `actions/checkout = "^4"` and an override `{ workflow = ".github/workflows/ci.yml", version = "^3" }`
- **WHEN** the manifest is written
- **THEN** the output is:
  ```toml
  [actions]
  "actions/checkout" = "^4"

  [actions.overrides]
  "actions/checkout" = [
    { workflow = ".github/workflows/ci.yml", version = "^3" },
  ]
  ```

#### Scenario: Manifest with actions only
- **GIVEN** a manifest with `actions/checkout = "^4"` and no overrides
- **WHEN** the manifest is written
- **THEN** the output contains `[actions]` section only (no empty `[actions.overrides]`)

---

## Lock Format

### Requirement: Lock file structure

The lock file SHALL have two top-level sections: `[resolutions]` and `[actions]`.

- `[resolutions]` maps `(ActionId, Specifier)` to a resolved version, using nested TOML tables keyed by action ID then specifier string.
- `[actions]` maps `(ActionId, Version)` to commit metadata (sha, repository, ref_type, date), using nested TOML tables keyed by action ID then version string.

#### Scenario: Standard lock with resolutions and actions
- **GIVEN** an action `actions/checkout@^6` resolved to SHA `de0fac2e...` at version `v6.2.3`, from a GitHub Release published at `2026-02-15T10:35:00Z`
- **THEN** the lock file is:
  ```toml
  [resolutions."actions/checkout"."^6"]
  version = "v6.2.3"

  [actions."actions/checkout"."v6.2.3"]
  sha = "de0fac2e..."
  repository = "actions/checkout"
  ref_type = "release"
  date = "2026-02-15T10:35:00Z"
  ```

#### Scenario: Subpath action stores base repository in actions tier
- **GIVEN** an action `github/codeql-action/upload-sarif@^3` resolved to version `v3.28.0` against repository `github/codeql-action`
- **THEN** the lock file contains:
  ```toml
  [resolutions."github/codeql-action/upload-sarif"."^3"]
  version = "v3.28.0"

  [actions."github/codeql-action/upload-sarif"."v3.28.0"]
  sha = "..."
  repository = "github/codeql-action"
  ref_type = "tag"
  date = "..."
  ```

#### Scenario: Multiple specifiers share one action entry
- **GIVEN** actions `actions/checkout@^4` and `actions/checkout@^4.2` both resolve to `v4.2.1`
- **THEN** the lock file has two resolution entries pointing to one action entry:
  ```toml
  [resolutions."actions/checkout"."^4"]
  version = "v4.2.1"

  [resolutions."actions/checkout"."^4.2"]
  version = "v4.2.1"

  [actions."actions/checkout"."v4.2.1"]
  sha = "abc123..."
  repository = "actions/checkout"
  ref_type = "tag"
  date = "2026-01-01T00:00:00Z"
  ```

#### Scenario: Non-semver branch ref in resolution
- **GIVEN** an action `actions/checkout@main` resolved to SHA `abc123...`
- **THEN** the resolution uses the branch name as both specifier key and version:
  ```toml
  [resolutions."actions/checkout"."main"]
  version = "main"

  [actions."actions/checkout"."main"]
  sha = "abc123..."
  repository = "actions/checkout"
  ref_type = "branch"
  date = "2026-03-01T00:00:00Z"
  ```

#### Scenario: Resolutions are sorted by action ID then specifier
- **GIVEN** resolutions for `actions/setup-node@^3` and `actions/checkout@^6`
- **WHEN** the lock is written
- **THEN** `actions/checkout` resolutions appear before `actions/setup-node` resolutions

#### Scenario: Actions are sorted by action ID then version
- **GIVEN** action entries for `actions/setup-node@v3.7.0` and `actions/checkout@v6.2.3`
- **WHEN** the lock is written
- **THEN** `actions/checkout` entries appear before `actions/setup-node` entries

### Requirement: Roundtrip integrity
Lock file serialization and deserialization SHALL be lossless for known fields across both tiers. The roundtrip baseline is the current format (without `comment`).

#### Scenario: Two-tier roundtrip
- **GIVEN** a two-tier lock file with resolutions (version only) and action entries
- **WHEN** the lock is read and then written back
- **THEN** the output is byte-for-byte identical to the input

#### Scenario: Legacy comment fields are dropped on write
- **GIVEN** a two-tier lock file with `comment` fields in resolution entries
- **WHEN** a write command (tidy, init, upgrade) runs
- **THEN** the `comment` fields are dropped silently
- **AND** this is a one-way migration (same pattern as flat-to-two-tier migration)
- **BECAUSE** the forward-compatible reads requirement ensures `comment` is ignored on parse; the write side produces the current format without `comment`

---

## Format Migration

### Requirement: Write-time migration
Migration SHALL occur transparently when a write command (tidy, init, upgrade) outputs files. Read-only commands (lint) do not modify files. The lock file is always written in the current two-tier format via full rewrite — no diff-based patching, no migration flags, no migration messages.

#### Scenario: Flat lock is silently migrated on tidy
- **WHEN** a flat-format lock file exists and `gx tidy` runs
- **THEN** the lock file is rewritten in two-tier format
- **AND** no "migrated" message is printed

#### Scenario: Flat lock is silently migrated on upgrade
- **WHEN** a flat-format lock file exists and `gx upgrade` runs
- **THEN** the lock file is rewritten in two-tier format
- **AND** no "migrated" message is printed

#### Scenario: Two-tier lock is rewritten identically
- **WHEN** a two-tier lock file exists and a write command runs with no logical changes
- **THEN** the lock file content is byte-for-byte identical after the write

#### Scenario: Lock save is always a full rewrite
- **WHEN** `Store::save()` is called
- **THEN** the entire `Lock` domain model is serialized to disk
- **AND** no prior file content is read or patched

### Requirement: Migration from flat lock to two-tier
The system SHALL transparently read flat-format lock files (single `[actions]` section with `"action@specifier"` composite keys) and convert them to the domain `Lock` model. The next write produces the two-tier format.

#### Scenario: Flat lock migrates to two-tier
- **GIVEN** a flat lock file:
  ```toml
  [actions."actions/checkout@^6"]
  sha = "de0fac2e..."
  version = "v6.2.3"
  comment = "v6"
  repository = "actions/checkout"
  ref_type = "release"
  date = "2026-02-15T10:35:00Z"
  ```
- **WHEN** a write command (tidy, init, upgrade) runs
- **THEN** the output is two-tier format without `comment`:
  ```toml
  [resolutions."actions/checkout"."^6"]
  version = "v6.2.3"

  [actions."actions/checkout"."v6.2.3"]
  sha = "de0fac2e..."
  repository = "actions/checkout"
  ref_type = "release"
  date = "2026-02-15T10:35:00Z"
  ```

#### Scenario: Flat entries deduplicating to one action entry
- **GIVEN** a flat lock file with two entries resolving to the same version:
  ```toml
  [actions."actions/checkout@^4"]
  sha = "abc123..."
  version = "v4.2.1"
  comment = "v4"
  repository = "actions/checkout"
  ref_type = "tag"
  date = "2026-01-01T00:00:00Z"

  [actions."actions/checkout@^4.2"]
  sha = "abc123..."
  version = "v4.2.1"
  comment = "v4.2"
  repository = "actions/checkout"
  ref_type = "tag"
  date = "2026-01-01T00:00:00Z"
  ```
- **WHEN** migration runs
- **THEN** the output has two resolution entries and one action entry

#### Scenario: Flat entry with missing version migrates with specifier fallback
- **GIVEN** a flat lock entry with `version` absent or empty
- **WHEN** migration runs
- **THEN** the resolution is created with the specifier as the version fallback
- **AND** the entry is detected as incomplete on next tidy run

#### Scenario: v1.4 inline tables parsed as flat format
- **GIVEN** a lock file with `version = "1.4"` and inline table entries under `[actions]`
- **WHEN** `Store::load()` is called
- **THEN** the file is parsed as flat format (the `version` field is ignored by serde)
- **AND** the entries are converted to the domain `Lock` model

#### Scenario: Empty lock file returns default
- **GIVEN** a lock file that exists but is empty
- **WHEN** `Store::load()` is called
- **THEN** an empty `Lock::default()` is returned

#### Scenario: Unrecognized format produces an error
- **GIVEN** a lock file with non-TOML content or unrecognized TOML structure
- **WHEN** `Store::load()` is called
- **THEN** an error is returned indicating the format is unrecognized

### Requirement: Manifest v1 migration
v1 manifests (no `[gx]` section with `"v4"` style values) are parsed directly with specifier values converted via `from_v1()`. v2 manifests (with `[gx]` section) have the section stripped. The `manifest_migrated` flag is preserved for manifest migration messages. The lock-specific `lock_migrated` flag and `Parsed<T>` wrapper for locks are removed; manifest migration uses a simpler mechanism.

#### Scenario: Manifest migration still reports
- **WHEN** a v1 manifest is loaded and a write command runs
- **THEN** the output includes `migrated gx.toml -> semver specifiers`

---

## Workflow Annotation

### Requirement: Workflow annotation shows resolved version

When gx writes a pinned action reference to a workflow file, the YAML comment SHALL show the resolved version from the lock, not a specifier-derived comment.

#### Scenario: Version annotation uses resolved version
- **GIVEN** a manifest specifier `^4` resolved to version `v4.2.1` with SHA `abc123...`
- **WHEN** gx writes the workflow file
- **THEN** the output is `uses: actions/checkout@abc123... # v4.2.1`
- **AND** NOT `uses: actions/checkout@abc123... # v4`

#### Scenario: Bare SHA specifier has no annotation
- **GIVEN** a manifest specifier that is a bare SHA
- **WHEN** gx writes the workflow file
- **THEN** the output is `uses: actions/checkout@abc123...` (no `# comment`)
