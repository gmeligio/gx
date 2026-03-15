## Why

Several domain concepts (repository identifiers, workflow paths, job IDs, version comments, commit dates, lint rule names) are represented as bare `String` primitives. This makes the domain model harder to reason about — function signatures don't communicate intent, nothing prevents mixing up two strings that mean different things, and the compiler can't catch semantic misuse. This is a refactoring to make the domain self-documenting through newtypes and enums.

## What Changes

Six incremental steps, each a standalone PR:

1. **`RuleName` enum** — Replace `rule: String` in `Diagnostic` and `BTreeMap<String, Rule>` in config with a 4-variant enum. Enables exhaustive matching, eliminates typo-class bugs.
2. **`Repository` newtype** — Wrap the `owner/repo` string in `Commit`, `ResolvedRef`, `ShaDescription`, and lock formats. `ActionId::base_repo()` returns `Repository` instead of `String`.
3. **`WorkflowPath` + `JobId` newtypes** — Wrap workflow and job fields in `Location` and `ActionOverride`, achieving type consistency with the existing `StepIndex` newtype. `WorkflowPath` enforces forward-slash normalization at construction.
4. **`VersionComment` newtype** — Wrap the derived version comment in `Resolution` and `Specifier::Range`. Encodes the invariant that comments are derived from specifiers, not arbitrary text.
5. **`CommitDate` newtype** — Wrap the ISO 8601 date string in `Commit`, `ResolvedRef`, and `ShaDescription`.
6. **`GitHubToken` newtype** — Wrap the GitHub token in config and registry. Masked `Debug` impl to structurally prevent accidental logging.

All newtypes use `String` internals. The goal is semantic clarity, not allocation optimization.

## Capabilities

### New Capabilities

None. This is an internal refactoring — no new user-facing behavior.

### Modified Capabilities

- **domain-composition spec**: `ResolvedCommit` field types change from `repository: String, date: String` to `repository: Repository, date: CommitDate`. Serialization format unchanged.
- **resolution spec**: `ShaDescription` field types change from `repository: String, date: String` to `repository: Repository, date: CommitDate`.
- **lint spec**: `RuleName` enum replaces string-based rule identification. Unrecognized rule names in `[lint.rules]` config now produce a parse error instead of being silently ignored.
- **file-format spec**: Clarifies that forward-compatible reads (ignore unknown keys) apply to `[resolutions]` and `[actions]` sections, not to `[lint.rules]`.
- **code-quality spec**: Adds newtype requirements for `WorkflowPath`, `JobId`, `VersionComment`, `CommitDate`, `GitHubToken` alongside existing `StepIndex` precedent.

## Impact

- **Domain layer** (`src/domain/`): Core struct fields change from `String` to newtypes. All construction sites and consumers update.
- **Infrastructure layer** (`src/infra/`): Serialization boundaries convert between wire `String` and domain newtypes.
- **Lint layer** (`src/lint/`): Rule identification switches from string matching to enum variants.
- **Test code**: Assertions update to use newtypes/enum variants instead of string literals.
- **No breaking changes to CLI, file formats, or public behavior.**
