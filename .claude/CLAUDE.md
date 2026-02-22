# CLAUDE.md

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
main.rs (Composition Root)
    ↓ Injects FileManifest/FileLock or MemoryManifest/MemoryLock
commands/tidy.rs (Application Layer)
    ↓ Uses ResolutionService + ManifestStore/LockStore traits
domain/ (Business Types + Resolution Logic)
infrastructure/ (File I/O + Github API)
```

```
src/
├── main.rs              # CLI entry, DI based on manifest existence
├── lib.rs               # Library root (re-exports commands, domain, infrastructure, config)
├── commands.rs          # Commands module (re-exports tidy)
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

When modifying code, update the corresponding documentation:
- `docs/` contains user-facing documentation (one file per command, plus manifest.md and lock.md)
- `docs/development/` contains contributor documentation (implementation details per command, plus architecture.md)
- Update both when changing command behavior, adding types, or modifying algorithms
- Keep excalidraw diagrams in `docs/development` in sync with architectural changes
