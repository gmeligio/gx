## Lock Format

### Requirement: Lock file structure [MODIFIED]

The lock file SHALL have two top-level sections: `[resolutions]` and `[actions]`.

- `[resolutions]` maps `(ActionId, Specifier)` to a resolved version, using nested TOML tables keyed by action ID then specifier string.
- `[actions]` maps `(ActionId, Version)` to commit metadata (sha, repository, ref_type, date), using nested TOML tables keyed by action ID then version string.

~~The `comment` field belongs in `[resolutions]` because it depends on specifier precision, not on the resolved version.~~ **REMOVED**: The `comment` field is removed. The resolved `version` field serves as the workflow annotation. The manifest specifier is the single source of intent; the lock stores only the resolved state.

#### Scenario: Standard lock with resolutions and actions [MODIFIED]
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

#### Scenario: Subpath action stores base repository in actions tier [MODIFIED]
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

#### Scenario: Multiple specifiers share one action entry [MODIFIED]
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

#### Scenario: Non-semver branch ref in resolution [MODIFIED]
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

### Requirement: Roundtrip integrity [MODIFIED]

Lock file serialization and deserialization SHALL be lossless for known fields across both tiers. The roundtrip baseline is the current format (without `comment`).

#### Scenario: Two-tier roundtrip
- **GIVEN** a two-tier lock file with resolutions (version only) and action entries
- **WHEN** the lock is read and then written back
- **THEN** the output is byte-for-byte identical to the input

#### Scenario: Legacy comment fields are dropped on write [NEW]
- **GIVEN** a two-tier lock file with `comment` fields in resolution entries
- **WHEN** a write command (tidy, init, upgrade) runs
- **THEN** the `comment` fields are dropped silently
- **AND** this is a one-way migration (same pattern as flat-to-two-tier migration)
- **BECAUSE** the forward-compatible reads requirement ensures `comment` is ignored on parse; the write side produces the current format without `comment`

---

## Format Migration

### Requirement: Migration from flat lock to two-tier [MODIFIED]

#### Scenario: Flat lock migrates to two-tier [MODIFIED]
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

---

## Workflow Annotation

### Requirement: Workflow annotation shows resolved version [NEW]

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
