# Design: Vertical Slicing

## Target Structure

```
src/
├── main.rs              ← CLI entry, clap, dispatch, spinner/log helpers
├── config.rs            ← Config, Settings, LintConfig
├── lib.rs               ← re-exports: domain, infra, output, tidy, init, upgrade, lint
│
├── domain/              ← SHARED: traits + types (unchanged name, gains command.rs + error.rs)
│   ├── mod.rs           ← pub use re-exports
│   ├── action/          ← identity.rs, spec.rs, resolved.rs, upgrade.rs, uses_ref.rs
│   ├── manifest.rs
│   ├── lock.rs
│   ├── workflow.rs      ← WorkflowScanner, WorkflowUpdater traits, WorkflowError
│   ├── workflow_actions.rs
│   ├── resolution.rs    ← VersionRegistry trait, ActionResolver, ShaIndex
│   ├── command.rs       ← Command trait, CommandReport trait (from command/traits.rs)
│   └── error.rs         ← AppError (from command/app.rs)
│
├── infra/               ← SHARED: I/O adapters (renamed from infrastructure/)
│   ├── mod.rs           ← pub use re-exports
│   ├── github.rs        ← GithubRegistry (implements VersionRegistry)
│   ├── workflow.rs      ← FileWorkflowScanner, FileWorkflowUpdater
│   ├── manifest.rs      ← parse_manifest, create_manifest, apply_manifest_diff
│   ├── lock.rs          ← parse_lock, create_lock, apply_lock_diff
│   └── repo.rs          ← find_root
│
├── tidy/
│   ├── mod.rs           ← Tidy struct, Command impl, plan(), apply_workflow_patches()
│   └── report.rs        ← TidyReport, CommandReport impl
│
├── init/
│   ├── mod.rs           ← Init struct, Command impl
│   └── report.rs        ← InitReport, CommandReport impl
│
├── upgrade/
│   ├── mod.rs           ← Upgrade struct, Command impl, plan(), resolve_upgrade_mode()
│   └── report.rs        ← UpgradeReport, CommandReport impl
│
├── lint/
│   ├── mod.rs           ← Lint struct, Command impl, collect_diagnostics()
│   ├── report.rs        ← LintReport, CommandReport impl
│   └── rules/
│       ├── mod.rs
│       ├── sha_mismatch.rs
│       ├── stale_comment.rs
│       ├── unpinned.rs
│       └── unsynced_manifest.rs
│
└── output/              ← PRESENTATION (slimmed — no more report.rs)
    ├── mod.rs
    ├── lines.rs         ← OutputLine enum
    ├── printer.rs       ← Printer
    └── log_file.rs      ← LogFile
```

## Module Dependency Rules

```
main.rs ──▶ config, domain, infra, tidy, init, upgrade, lint, output

tidy/     ──▶ domain, infra, output
init/     ──▶ domain, infra, tidy (for tidy::plan), output
upgrade/  ──▶ domain, infra, output
lint/     ──▶ domain, infra, output, config (for LintConfig)

config    ──▶ domain, infra
infra/    ──▶ domain
domain/   ──▶ (nothing — leaf)
output/   ──▶ (nothing — leaf, except OutputLine is used everywhere)
```

Key rule: **Feature modules never import from each other**, except `init → tidy::plan` (init is a thin wrapper around tidy's planning logic). This is an explicit, documented exception.

## What Moves Where

### domain/ — stays, gains two files

`domain/` keeps its name and all existing files. Two files move in:

| From | To |
|---|---|
| `command/traits.rs` | `domain/command.rs` |
| `command/app.rs` | `domain/error.rs` |

### infrastructure/ → infra/

Pure rename:

| From | To |
|---|---|
| `infrastructure/github.rs` | `infra/github.rs` |
| `infrastructure/workflow.rs` | `infra/workflow.rs` |
| `infrastructure/manifest.rs` | `infra/manifest.rs` |
| `infrastructure/lock.rs` | `infra/lock.rs` |
| `infrastructure/repo.rs` | `infra/repo.rs` |
| `infrastructure/mod.rs` | `infra/mod.rs` (updated module path) |

### domain/plan.rs → split

`TidyPlan`, `WorkflowPatch`, `LockEntryPatch`, `ManifestDiff`, `LockDiff` are currently all in `domain/plan.rs`.

- `TidyPlan`, `WorkflowPatch`, `LockEntryPatch` → `tidy/mod.rs` (only used by tidy + init)
- `UpgradePlan` → `upgrade/mod.rs` (only used by upgrade)
- `ManifestDiff`, `LockDiff` → `domain/plan.rs` (stays, but only shared diff types remain)

### output/report.rs → split per feature

| Type | Moves to |
|---|---|
| `TidyReport` | `tidy/report.rs` |
| `InitReport` | `init/report.rs` |
| `UpgradeReport` | `upgrade/report.rs` |
| `LintReport` | `lint/report.rs` |

Each report implements `CommandReport` from `domain::command`.

### command/ → dissolved

| From | To |
|---|---|
| `command/traits.rs` | `domain/command.rs` |
| `command/app.rs` | `domain/error.rs` |
| `command/common.rs` | `main.rs` (inlined — only used there) |
| `command/tidy.rs` | `tidy/mod.rs` |
| `command/init.rs` | `init/mod.rs` |
| `command/upgrade.rs` | `upgrade/mod.rs` |
| `command/lint/` | `lint/` |
| `command/mod.rs` | deleted |

## Worktree Safety Analysis

After this refactor, here's what each feature touches:

| Feature | Owned files | Shared files read |
|---|---|---|
| tidy | `tidy/mod.rs`, `tidy/report.rs` | `domain/*`, `infra/*`, `output/*` |
| init | `init/mod.rs`, `init/report.rs` | `domain/*`, `infra/*`, `tidy/mod.rs` |
| upgrade | `upgrade/mod.rs`, `upgrade/report.rs` | `domain/*`, `infra/*`, `output/*` |
| lint | `lint/mod.rs`, `lint/report.rs`, `lint/rules/*` | `domain/*`, `infra/*`, `output/*` |

Two agents working on `tidy/` and `upgrade/` will **never touch the same files** unless they're modifying shared traits in `domain/`, which should be rare and deliberate.

## Migration Strategy

This is a pure move/rename refactor. The approach is:

1. Rename `infrastructure/` → `infra/` and update imports
2. Move `command/traits.rs` and `command/app.rs` into `domain/`
3. Split `plan.rs` and `report.rs` into feature modules
4. Promote each command to a top-level feature module
5. Inline `command/common.rs` into `main.rs` and delete `command/`

No logic changes. No behavior changes. The diff will be large but mechanical.
