## Why

Four infra files exceed the 500-line budget: `manifest.rs` (1445), `lock.rs` (867), `github.rs` (835), and `workflow_scan.rs` (627). These files mix serialization wire types, conversion logic, file I/O, surgical TOML patching, and tests in single monoliths.

## What Changes

- **Split `infra/manifest.rs`** into a `manifest/` directory: wire types + conversion, TOML patching + formatting, and the main module with file I/O and public APIs.
- **Split `infra/lock.rs`** into a `lock/` directory: wire types + conversion, and the main module with file I/O and diff application.
- **Split `infra/github.rs`** into a `github/` directory: response DTOs, resolution/tag logic, and the main module with client + trait impl.
- **Split `infra/workflow_scan.rs`** tests into a separate file to bring code under budget.
- Update `infra/mod.rs` re-exports.

## Capabilities

### New Capabilities

_(None — purely structural.)_

### Modified Capabilities

_(No existing capabilities are changing — these are internal structural improvements.)_

## Impact

- **Infra layer** (`src/infra/`): 4 flat files become 3 directories + 1 slimmed file. All public APIs unchanged.
- **No changes to domain, command, or test layers** (except infra test redistribution).
- **No user-facing changes**: CLI behavior, manifest format, and lock format are unchanged.
