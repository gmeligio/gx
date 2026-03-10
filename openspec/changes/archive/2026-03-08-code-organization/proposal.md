## Why

The codebase has grown organically and shows signs of an anemic domain model. Business logic (`diff_manifests`, `diff_locks`, `sync_overrides`, etc.) lives in command modules (`tidy/`, `upgrade/`) rather than on the domain types they operate on. This has led to duplicated functions with divergent behavior, 2000-line files, and tests that require filesystem fixtures for what should be pure in-memory domain logic. There are no guardrails to prevent further architectural drift.

## What Changes

- **Enrich domain types**: Move business logic from command modules onto `Manifest` and `Lock` domain types. Domain methods return `Vec<SyncEvent>` instead of taking `on_progress` callbacks — making them pure, testable, and presentation-free.
- **Eliminate duplication**: Unify `diff_manifests()` and `diff_locks()` (duplicated in tidy and upgrade with divergent behavior) into `Manifest::diff()` and `Lock::diff()` domain methods.
- **Add architecture guardrails**: Tests in `code_health.rs` that enforce layer dependency direction, detect duplicate functions across commands, and budget file/folder sizes. A `mise` task wired into `clippy` for file size checks.
- **Split large files**: Break `tidy/mod.rs` (1970 lines) into semantic submodules. Split `infra/lock.rs` and `infra/workflow.rs` along natural seams.
- **Replace `on_progress` callback with `SyncEvent`**: Domain methods return structured events; command orchestrators format and display them independently.

## Capabilities

### New Capabilities

- `architecture-guardrails`: Code health tests and mise tasks that enforce layer boundaries, detect anemic domain patterns (duplicate functions across commands), and budget file/folder sizes.
- `domain-sync-events`: A `SyncEvent` enum returned by domain methods, replacing the `on_progress: &mut dyn FnMut(&str)` callback pattern for domain operations.

### Modified Capabilities

_(No existing spec-level capabilities are changing — these are internal structural improvements.)_

## Impact

- **Domain layer** (`src/domain/`): `Manifest` and `Lock` gain new methods. New `SyncEvent` type. `ActionResolver`/`VersionRegistry` may be restructured for simpler method signatures.
- **Command modules** (`src/tidy/`, `src/upgrade/`): Become thinner orchestrators. Remove duplicated helper functions. Tidy splits into submodules.
- **Infra layer** (`src/infra/`): Lock migration logic extracted. Workflow scanner/updater split into separate files.
- **Tests**: Many tidy integration tests (filesystem-based) can become simpler domain unit tests. New architecture tests in `code_health.rs`.
- **CI**: `mise run clippy` gains file size checking.
- **No user-facing changes**: CLI behavior, manifest format, and lock format are unchanged.
