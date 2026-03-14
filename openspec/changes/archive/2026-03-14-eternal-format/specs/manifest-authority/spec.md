## MODIFIED Requirements

### Requirement: Manifest has no gx section
The manifest SHALL NOT contain a `[gx]` section. The file format is `[actions]` with optional `[actions.overrides]` and `[lint]` sections.

#### Scenario: Written manifest has no gx section
- **GIVEN** a manifest written by the current gx version
- **THEN** the file does not contain a `[gx]` section or `min_version` field

### Requirement: Manifest writes use toml_edit
The manifest write path SHALL use `toml_edit::DocumentMut` to produce output. Overrides SHALL be written as inline tables within arrays.

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

## REMOVED Requirements

### Requirement: Manifest v1 to v2 migration
**Reason**: The `[gx]` section no longer exists. There is no v2 format to migrate to.
**Migration**: v1 manifests (no `[gx]` section with `"v4"` style values) are parsed directly — specifier values are converted via `from_v1()`. v2 manifests (with `[gx]` section) have the section stripped. Both set `migrated = true`.
