# Infra Module Colocation

## Summary

Extract logic from `mod.rs` files across all four `infra/` submodules so that each `mod.rs` becomes reexports only.

## Motivation

Four `infra/` submodules have business logic in `mod.rs`:

| File | Logic lines | What's in there |
|------|------------|-----------------|
| `infra/github/mod.rs` | 264 | Registry struct, Error enum, HTTP client setup |
| `infra/manifest/mod.rs` | 203 | TOML parsing, Error enum |
| `infra/lock/mod.rs` | 135 | Store struct, Error enum |
| `infra/workflow_scan/mod.rs` | 308 | FileScanner, YAML parsing |

These are grouped into one proposal because they follow the same pattern: extract logic into a semantically-named file, leave `mod.rs` as reexports.

## Spec gate

Skipped — internal restructuring with no user-visible change.

## Changes

### 1. `infra/github/` — create `registry.rs`

Move from `mod.rs`:
- `Error` enum
- `Registry` struct + constructor + HTTP helpers
- `impl VersionRegistry for Registry`

`mod.rs` becomes:

```rust
mod registry;
mod resolve;
mod responses;

pub use registry::{Error, Registry};
```

Also: inline `infra/github/tests.rs` (98 lines) into the bottom of `resolve.rs` as a `#[cfg(test)] mod tests` block. The tests exercise `resolve.rs` functions — they should live there, not be `#[path]`-included from a separate file.

### 2. `infra/manifest/` — create `parse.rs`

Move from `mod.rs`:
- `MANIFEST_FILE_NAME` constant
- `Error` enum
- `Store` struct + `new()` + `save()`
- `parse()`, `parse_lint_config()`, `create()` functions

`mod.rs` becomes:

```rust
mod convert;
pub mod patch;
mod parse;

pub use parse::{Error, Store, parse, parse_lint_config, create, MANIFEST_FILE_NAME};
```

Existing `tests.rs` stays as `#[path = "tests.rs"]` include from `parse.rs` (tests exercise the parsing logic).

### 3. `infra/lock/` — create `store.rs`

Move from `mod.rs`:
- `Error` enum
- `Store` struct + `load()`/`save()` methods

`mod.rs` becomes:

```rust
mod format;
mod migration;
mod store;

pub use store::{Error, Store};
```

Existing `tests.rs` stays as `#[path = "tests.rs"]` include from `store.rs`.

### 4. `infra/workflow_scan/` — flatten or create `scanner.rs`

Two options:

**Option A: Flatten to `infra/workflow_scan.rs`** — since there's only one concept (FileScanner), the directory is unnecessary. Inline tests at the bottom. Total: ~670 lines (300 logic + 361 tests). Exceeds 550-line total budget.

**Option B: Create `scanner.rs`** — keep the directory, move logic to `scanner.rs`, leave `mod.rs` as reexports. Tests stay as `#[path = "tests.rs"]` from `scanner.rs`.

Recommend **Option B** — the test file is too large for flattening to work within the total line budget.

## Ordering

The four extractions are independent and can land in any order or together. Recommend doing them in a single commit since they're all mechanical.

## Risks

- Reexports (`pub use`) must maintain the same public API surface so that `crate::infra::github::Registry` etc. continue to work unchanged.
- `infra/github/tests.rs` inline is the only non-mechanical step — verify test imports update correctly.
