## Domain Type Composition

### Requirement: Spec replaces LockKey

The `LockKey` type SHALL be deleted. `Spec` SHALL be used everywhere a lock lookup key is needed. `Spec` SHALL derive `Hash` and `Eq` to support use as a `HashMap` key.

#### Scenario: Spec used as lock key
- **GIVEN** a manifest spec `actions/checkout@^6`
- **WHEN** looking up the corresponding lock entry
- **THEN** the lookup uses a `Spec` value directly (no `From<&Spec> for LockKey` conversion)

#### Scenario: Spec parsing from lock key format
- **WHEN** a lock file key `"actions/checkout@^6"` is parsed
- **THEN** `Spec::parse("actions/checkout@^6")` SHALL return a `Spec` with `id = "actions/checkout"` and `version = "^6"`

#### Scenario: Specifier derives Hash and Eq
- **GIVEN** `Spec` derives `Hash` and `Eq`
- **THEN** `Specifier` SHALL also derive `Hash` and `Eq`
- **AND** two `Specifier` values parsed from the same string SHALL be equal

#### Scenario: RegistryResolution to Spec conversion
- **WHEN** a `RegistryResolution` needs a lock key
- **THEN** `From<&RegistryResolution> for Spec` SHALL produce a `Spec` from the resolution's `id` and `specifier`

### Requirement: ResolvedCommit extracts shared commit metadata

A `ResolvedCommit` struct SHALL hold the four fields shared between `Resolved` and `Entry`:

```
ResolvedCommit { sha: CommitSha, repository: Repository, ref_type: Option<RefType>, date: CommitDate }
```

#### Scenario: RegistryResolution uses ResolvedCommit
- **GIVEN** a registry resolution
- **THEN** `RegistryResolution` SHALL have fields `{ id: ActionId, specifier: Specifier, commit: ResolvedCommit }`
- **AND** accessing the SHA is `resolution.commit.sha`

#### Scenario: Entry uses ResolvedCommit
- **GIVEN** a lock entry
- **THEN** `Entry` SHALL have fields `{ commit: ResolvedCommit, version: Option<String>, comment: VersionComment }`
- **AND** accessing the repository is `entry.commit.repository`

#### Scenario: with_sha consumes self
- **WHEN** `Resolved::with_sha(self, sha)` is called
- **THEN** it SHALL consume `self` and return a new `Resolved` with the SHA replaced
- **AND** no `.clone()` calls SHALL appear in the method body

#### Scenario: Entry constructed from ResolvedCommit
- **WHEN** a lock entry is created from a resolution result
- **THEN** the `ResolvedCommit` SHALL be moved into the `Entry` (not field-by-field cloned)

#### Scenario: Lock file TOML format unchanged by ResolvedCommit
- **WHEN** an `Entry` containing a `ResolvedCommit` is serialized to TOML
- **THEN** the fields `sha`, `repository`, `ref_type`, `date` SHALL appear as flat keys in the TOML table (not nested under a `commit` key)
- **AND** existing lock files SHALL parse and roundtrip identically

#### Scenario: Lock actions tier uses Commit with newtypes
- **GIVEN** the lock's actions tier maps `ActionKey` → `Commit`
- **THEN** `Commit::repository` SHALL be `Repository` (not `String`)
- **AND** `Commit::date` SHALL be `CommitDate` (not `String`)

#### Scenario: Resolution comment updated to VersionComment
- **GIVEN** the lock's `Resolution` struct has a `comment` field
- **THEN** `Resolution::comment` SHALL be `VersionComment` (not `String`)
- **AND** `Resolution.version` remains `Version` (unchanged)
- **NOTE** The main spec's `Entry { commit: ResolvedCommit, version: Option<String>, comment: String }` is aspirational naming. In the codebase, `Resolution` holds the `comment` field, and `Commit` holds commit metadata.

### Requirement: Located composes InterpretedRef

`Located` SHALL compose `InterpretedRef` instead of duplicating its fields:

```
Located { action: InterpretedRef, location: Location }
```

#### Scenario: Located provides access to action fields
- **GIVEN** a `Located` value
- **WHEN** accessing the action ID
- **THEN** the caller uses `located.action.id`

#### Scenario: ActionSet accepts InterpretedRef
- **WHEN** adding an action to `ActionSet`
- **THEN** `ActionSet::add` SHALL accept `&InterpretedRef`
- **AND** `ActionSet` SHALL NOT have a separate `add_located` method
- **AND** callers with a `Located` pass `&located.action`

#### Scenario: ActionSet::from_located uses composition
- **WHEN** building an `ActionSet` from located actions
- **THEN** `from_located` SHALL access fields via `action.action.id` and `action.action.version`

---

## Ownership-First Signatures

### Requirement: Functions consuming all fields take ownership

Functions that clone every field of a borrowed parameter SHALL instead take the parameter by value (ownership transfer).

#### Scenario: ActionSet::add borrows InterpretedRef
- **WHEN** `ActionSet::add` needs to store the action's ID and version
- **THEN** it SHALL accept `&InterpretedRef` (borrowing is acceptable here because only `id` and `version` are inserted into HashMaps which require owned values, and `InterpretedRef` may be used after the call)

#### Scenario: RegistryResolution::with_sha consumes self
- **WHEN** replacing the SHA on a registry resolution
- **THEN** `with_sha(self, sha: CommitSha) -> Self` SHALL consume `self`
- **AND** the caller SHALL not use the original value after the call

#### Scenario: ActionSet::from_located takes owned Vec
- **WHEN** building an `ActionSet` from discovered actions
- **THEN** `from_located(actions: Vec<Located>) -> Self` SHALL take ownership of the vector
- **AND** the caller SHALL not use the vector after the call

---

## Domain-Typed Reports

### Requirement: Report structs use domain types instead of strings

Report structs SHALL use domain identity types (`ActionId`, `Specifier`, `Version`) instead of raw `String` for action-related fields. Display formatting SHALL remain in the `render()` method.

#### Scenario: TidyReport uses ActionId
- **GIVEN** a tidy report
- **THEN** `removed` SHALL be `Vec<ActionId>`
- **AND** `added` SHALL be `Vec<(ActionId, Specifier)>`
- **AND** `upgraded` SHALL be `Vec<(ActionId, String, String)>` (from/to remain strings as they are display values)

#### Scenario: UpgradeReport uses ActionId
- **GIVEN** an upgrade report
- **THEN** `upgrades` SHALL be `Vec<(ActionId, String, String)>` (action is domain type, from/to are display strings)
- **AND** `skipped` SHALL be `Vec<(ActionId, String)>`

#### Scenario: Render produces String output
- **WHEN** `render()` is called on a report
- **THEN** `ActionId` and `Specifier` values SHALL be formatted via `Display` or `.as_str()` into the `OutputLine` string fields
- **AND** no `.clone()` on inner strings SHALL be needed at the report-building boundary

---

## Serialization Boundary

### Requirement: WorkflowPatch carries domain types

`WorkflowPatch.pins` SHALL be `Vec<ResolvedAction>` instead of `Vec<(ActionId, String)>`. The domain plan struct SHALL NOT carry pre-formatted serialization strings.

#### Scenario: WorkflowPatch uses ResolvedAction
- **GIVEN** a tidy plan produces workflow patches
- **WHEN** `WorkflowPatch` is constructed
- **THEN** `pins` contains `ResolvedAction` values with `id`, `sha`, and `version`
- **AND** no `"SHA # version"` string formatting has occurred yet

### Requirement: Updater trait deleted

The `Updater` trait in `domain/workflow.rs` SHALL be deleted. It has exactly one implementation (`FileUpdater`), is never mocked, and its `HashMap<ActionId, String>` signature forces callers to pre-format serialization strings. Callers SHALL use `WorkflowWriter` (renamed from `FileUpdater`) directly from the infra layer.

### Requirement: FileUpdater renamed to WorkflowWriter

`FileUpdater` in `infra/workflow_update.rs` SHALL be renamed to `WorkflowWriter`. It SHALL accept `&[WorkflowPatch]` instead of `HashMap<ActionId, String>`.

### Requirement: Single serialization point for YAML comment syntax

The `"SHA # version"` YAML comment formatting SHALL appear in exactly one function: `format_uses_ref()` in `infra/workflow_update.rs`. No other module SHALL construct this format string.

#### Scenario: format_uses_ref with version annotation
- **GIVEN** a `ResolvedAction` with `sha = "abc123..."` and `version = Some("v4.2.1")`
- **WHEN** `format_uses_ref()` is called
- **THEN** the output is `"abc123... # v4.2.1"`

#### Scenario: format_uses_ref with bare SHA (no annotation)
- **GIVEN** a `ResolvedAction` with `sha = "abc123..."` and `version = None`
- **WHEN** `format_uses_ref()` is called
- **THEN** the output is `"abc123..."` (no `#` comment)

### Requirement: Lock::build_update_map deleted

`Lock::build_update_map()` SHALL be deleted. It is dead code in production (never called from any command path). It also leaks serialization by producing `"SHA # version"` formatted strings.

**Note**: This method is used in `tests/integ_upgrade.rs` — that test must be updated to use the new workflow output path instead.
