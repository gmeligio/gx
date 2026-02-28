## Why

`gx tidy` keeps workflows in sync, but there's no way to **verify** that sync without running tidy itself (which mutates files). Users need a read-only check they can run in CI, pre-commit hooks, or locally to catch drift before it causes problems. A `gx lint` command provides this guardrail.

Beyond drift detection, a lint framework creates a natural extension point for future checks: CVE scanning, deprecated action detection, runner version auditing, etc.

## What Changes

- New `gx lint` subcommand: read-only, network-free, exits non-zero on errors
- New `[lint.rules]` section in `gx.toml` for per-rule configuration (level + ignore)
- Four lint rules ship in v1: `sha-mismatch`, `unpinned`, `unsynced-manifest`, `stale-comment`
- Zero-config works: no `[lint]` section means all rules run at their default levels
- Each rule is independent, validates exactly one thing, and is individually configurable
- Ignore targets use typed keys (`action`, `workflow`, `job`) — the same domain terms as action overrides

## Capabilities

### New Capabilities

- `lint-command`: Run read-only lint checks against workflows, manifest, and lock
- `lint-config`: Configure lint rule severity and ignore targets in `gx.toml`
- `lint-rule-sha-mismatch`: Detect when workflow SHAs don't match lock file
- `lint-rule-unpinned`: Detect action references using tags instead of SHA pins
- `lint-rule-unsynced-manifest`: Detect manifest/workflow action set divergence
- `lint-rule-stale-comment`: Detect version comments that don't match lock entries

### Modified Capabilities

<!-- none -->

## Impact

- `crates/gx/src/main.rs`: New `Lint` variant in `Commands` enum, dispatch to lint orchestrator
- `crates/gx-lib/src/commands/`: New `lint.rs` module with orchestrator and rule trait
- `crates/gx-lib/src/commands/lint/`: Rule implementations (one file per rule)
- `crates/gx-lib/src/config.rs`: Parse `[lint.rules]` section into `LintConfig`
- `crates/gx-lib/src/infrastructure/manifest.rs`: Extend TOML wire types for `[lint.rules]`
- No changes to tidy, upgrade, or init commands
- No changes to manifest/lock/workflow write paths
