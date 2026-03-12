## Why

Type names like `TidyError`, `LintError`, `UpgradePlan`, `GithubRegistry` repeat their module name. The idiomatic Rust convention is to use the module path as the qualifier: `tidy::Error`, `lint::Error`, `upgrade::Plan`, `github::Registry`. This makes names concise at the definition site and clear at the usage site. Enabling the `module_name_repetitions` and `pub_use` clippy lints enforces this convention and encourages colocation of related types in their natural module.

## What Changes

- **BREAKING**: Rename ~47 types to remove module name prefix/suffix
- Remove all `pub use` re-exports from facade modules (`domain/mod.rs`, `infra/mod.rs`, etc.)
- Update all import paths across the codebase to use qualified module paths
- Deny `clippy::module_name_repetitions` and `clippy::pub_use` lints in `Cargo.toml`

## Capabilities

### New Capabilities
- `module-qualified-types`: Defines the naming convention where types drop the module name prefix/suffix and are accessed via qualified paths (e.g., `tidy::Plan` instead of `TidyPlan`).

### Modified Capabilities
- `architecture-guardrails`: Adds `module_name_repetitions` and `pub_use` to the lint configuration. Import path hygiene rules may need adjustment for the new qualified import style.

## Impact

- **All `src/**/*.rs` files**: Type renames and import path changes
- **All `tests/**/*.rs` files**: Import path changes
- **Cargo.toml**: Two new lint entries
- **Breaking for downstream (if any)**: Type names change. Since this is a CLI binary (not a library), there are no external consumers.
- **No behavioral changes**: Only names and import paths change; all logic remains identical.
