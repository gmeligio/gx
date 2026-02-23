# AGENTS.md

## Project Overview

gx is a Rust CLI that manages Github Actions dependencies across workflows, similar to `go mod tidy`. It can maintain `.github/gx.toml` (manifest) and `.github/gx.lock` (resolved SHAs), or run in memory-only mode.

## Commands

```bash
cargo build                    # Build
cargo clippy                   # Lint
cargo test                     # Run all tests
cargo run -- init              # Create manifest and lock from workflows
cargo run -- tidy              # Sync manifest/lock with workflows
cargo run -- upgrade           # Check for newer versions and upgrade
```

## Architecture

```
main.rs (Presentation Layer)
    ↓ CLI parsing and forwarding
commands/app.rs (Application Dispatcher)
    ↓ Store construction and command dispatch
commands/tidy.rs, upgrade.rs (Application Layer)
    ↓ Uses ManifestStore/LockStore/VersionRegistry traits
domain/ (Business Types + Resolution Logic)
infrastructure/ (File I/O + Github API)
```

```
src/
├── main.rs              # Presentation layer: CLI parsing and arg forwarding
├── lib.rs               # Library root (re-exports commands, domain, infrastructure, config)
├── commands.rs          # Commands module
├── commands/app.rs      # Application dispatcher: store construction + command dispatch
├── commands/tidy.rs     # Generic run<M: ManifestStore, L: LockStore>()
├── commands/upgrade.rs  # Upgrade actions to newer versions
├── config.rs            # Environment configuration
├── domain/
│   ├── mod.rs           # Public re-exports for domain types
│   ├── action.rs        # ActionId, Version, CommitSha, ActionSpec, ResolvedAction, LockKey, UsesRef, InterpretedRef, VersionCorrection
│   ├── resolution.rs    # VersionRegistry trait, ActionResolver, ResolutionResult, select_best_tag
│   └── workflow_actions.rs # WorkflowActionSet (aggregates actions across workflows)
└── infrastructure/
    ├── mod.rs           # Public re-exports for infrastructure types
    ├── github.rs        # GithubRegistry (implements VersionRegistry)
    ├── lock.rs          # LockStore trait, FileLock, MemoryLock
    ├── manifest.rs      # ManifestStore trait, FileManifest, MemoryManifest
    ├── repo.rs          # Repository discovery (gix-discover)
    └── workflow.rs      # WorkflowParser, WorkflowWriter, UpdateResult
```

### Tidy Modes

- **File-backed** (manifest exists): Uses `FileManifest`/`FileLock`, persists to disk
- **Memory-only** (no manifest): Uses `MemoryManifest`/`MemoryLock`, updates workflows only

### Init Command

`init` creates manifest and lock files from current workflows. Errors if manifest already exists (use `tidy` to update instead).

### Tidy Data Flow

1. Find repo root (requires `.github` folder)
2. `WorkflowParser::scan_all()` extracts `UsesRef` from workflows, interprets into `InterpretedRef`, aggregates into `WorkflowActionSet`
3. Sync manifest: remove unused, add missing (highest semver wins via `select_highest_version`)
4. Resolve versions to SHAs via `ResolutionService`
5. Update lock and workflows (persisted only in file-backed mode)

## Code Patterns

### Domain Types
- Strong types prevent string mix-ups: `ActionId`, `Version`, `CommitSha`
- `UsesRef` → raw parsed data from workflow YAML (action_name, uses_ref, comment)
- `InterpretedRef` → result of `UsesRef::interpret()` with normalized version and optional SHA
- `WorkflowActionSet` → aggregates actions across all workflows, deduplicates versions, first SHA wins
- `VersionCorrection` → tracks when a SHA doesn't match its version comment
- `LockKey` centralizes `action@version` composite key format
- `ResolutionResult` has three variants: `Resolved`, `Corrected`, `Unresolved`

### Dependency Injection
- `main.rs` checks if `gx.toml` exists and injects appropriate implementations
- `tidy::run<M: ManifestStore, L: LockStore>()` works with trait abstractions
- `GithubClient` implements `VersionResolver` for testable resolution

### Error Handling
- Module-specific error enums with `thiserror` (e.g., `ManifestError`, `LockFileError`, `WorkflowError`)
- `anyhow::Result<T>` in commands for error propagation
- Graceful degradation: missing `GITHUB_TOKEN` logs warnings but continues

### File I/O
- `FileManifest`/`FileLock` track `path` and `dirty: bool`
- Use `load_or_default()` / `save()` for idempotency; only writes if dirty
- `MemoryManifest`/`MemoryLock` have no-op `save()`

### YAML Version Extraction
Two-phase approach (YAML parsers strip comments):
1. Scan raw content for version comments (`uses: action@SHA # v4`) into `UsesRef`
2. Parse YAML with `serde-saphyr` and merge with extracted comments
3. `UsesRef::interpret()` normalizes versions and identifies SHAs

## Logging

`env_logger` with `--verbose` flag. Default level is `Info`; `--verbose` enables `Debug`.

## Environment

- `GITHUB_TOKEN`: Optional, needed for Github API when resolving SHAs

## Documentation

**Documentation updates are mandatory and part of the definition of done.** A task is not complete until the relevant docs are updated. Do not consider a task finished, do not commit, and do not summarize results until documentation is updated.

### Doc file map

| Changed code | Update these docs |
|---|---|
| `src/commands/tidy.rs` | `docs/tidy.md`, `docs/development/tidy.md` |
| `src/commands/upgrade.rs` | `docs/upgrade.md`, `docs/development/upgrade.md` |
| `src/commands/init.rs` or init logic in `main.rs` | `docs/init.md`, `docs/development/init.md` |
| `src/infrastructure/manifest.rs` | `docs/manifest.md` |
| `src/infrastructure/lock.rs` | `docs/lock.md` |
| `src/domain/action.rs`, `src/domain/resolution.rs`, `src/domain/workflow_actions.rs` | `docs/development/architecture.md` |
| Any architectural change (new types, new traits, new data flow) | `docs/development/architecture.md` and relevant excalidraw diagrams |
| CLI flags, command behavior, output format | User-facing doc in README.md and `docs/<command>.md` |
| Internal algorithm, data flow, type changes | Developer doc in `docs/development/<command>.md` |

### Rules

1. After every code change, identify which rows in the table above apply and update those files.
2. If a new command is added, create both `docs/<command>.md` (user-facing) and `docs/development/<command>.md` (implementation).
3. If a type, trait, or algorithm is renamed or restructured, update all references in `docs/development/architecture.md`.
4. Keep excalidraw diagrams in `docs/development/` in sync with architectural changes (data flow, new layers, new types).
5. Do not add placeholder text. Write accurate, concrete documentation reflecting the actual current behavior.
