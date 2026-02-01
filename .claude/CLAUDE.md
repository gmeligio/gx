# CLAUDE.md

## Project Overview

gx is a Rust CLI that manages GitHub Actions dependencies across workflows, similar to `go mod tidy`. It can maintain `.github/gx.toml` (manifest) and `.github/gx.lock` (resolved SHAs), or run in memory-only mode.

## Commands

```bash
cargo build                    # Build
cargo clippy                   # Lint
cargo test                     # Run all tests
cargo run -- freeze            # Create manifest and lock from workflows
cargo run -- tidy              # Sync manifest/lock with workflows
```

## Architecture

```
main.rs (Composition Root)
    ↓ Injects FileManifest/FileLock or MemoryManifest/MemoryLock
commands/tidy.rs (Application Layer)
    ↓ Uses ResolutionService + ManifestStore/LockStore traits
domain/ (Business Types + Resolution Logic)
infrastructure/ (File I/O + GitHub API)
```

```
src/
├── main.rs              # CLI entry, DI based on manifest existence
├── commands/tidy.rs     # Generic run<M: ManifestStore, L: LockStore>()
├── domain/
│   ├── action.rs        # ActionId, Version, CommitSha, ActionSpec, ResolvedAction, LockKey
│   ├── resolution.rs    # VersionResolver trait, ResolutionService, ResolutionResult
│   └── version.rs       # Semver parsing, is_commit_sha()
├── infrastructure/
│   ├── github.rs        # GitHubClient (implements VersionResolver)
│   ├── lock.rs          # LockStore trait, FileLock, MemoryLock
│   ├── manifest.rs      # ManifestStore trait, FileManifest, MemoryManifest
│   ├── repo.rs          # Repository discovery (gix-discover)
│   └── workflow.rs      # WorkflowParser, WorkflowWriter, ExtractedAction
└── config.rs            # Environment configuration
```

### Tidy Modes

- **File-backed** (manifest exists): Uses `FileManifest`/`FileLock`, persists to disk
- **Memory-only** (no manifest): Uses `MemoryManifest`/`MemoryLock`, updates workflows only

### Tidy Data Flow

1. Find repo root (requires `.github` folder)
2. Extract actions from `.github/workflows/*.yml`
3. Sync manifest: remove unused, add missing (highest semver wins)
4. Resolve versions to SHAs via `ResolutionService`
5. Update lock and workflows (persisted only in file-backed mode)

## Code Patterns

### Domain Types
- Strong types prevent string mix-ups: `ActionId`, `Version`, `CommitSha`
- `LockKey` centralizes `action@version` composite key format
- `ResolutionService` encapsulates version resolution logic

### Dependency Injection
- `main.rs` checks if `gx.toml` exists and injects appropriate implementations
- `tidy::run<M: ManifestStore, L: LockStore>()` works with trait abstractions
- `GitHubClient` implements `VersionResolver` for testable resolution

### Error Handling
- Module-specific error enums with `thiserror` (e.g., `ManifestError`, `LockFileError`)
- `anyhow::Result<T>` in commands for error propagation
- Graceful degradation: missing `GITHUB_TOKEN` logs warnings but continues

### File I/O
- `FileManifest`/`FileLock` track `path` and `changed: bool`
- Use `load_or_default()` / `save()` for idempotency
- `MemoryManifest`/`MemoryLock` have no-op `save()`

### YAML Version Extraction
Two-phase approach (YAML parsers strip comments):
1. Scan raw content for version comments (`uses: action@SHA # v4`)
2. Parse YAML and merge with extracted comments

## Logging

`env_logger` with `--verbose` flag. Default level is `Info`; `--verbose` enables `Debug`.

## Environment

- `GITHUB_TOKEN`: Optional, needed for GitHub API when resolving SHAs
