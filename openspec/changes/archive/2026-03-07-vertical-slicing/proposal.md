## Why

The current horizontal slicing (command/, domain/, infrastructure/, output/) means that adding or modifying a single feature (e.g., tidy) touches files across 4 directories. Shared files like `domain/plan.rs` (contains both TidyPlan and UpgradePlan) and `output/report.rs` (contains all report types) are merge conflict hotspots. This blocks worktree-based parallel development — two agents working on different features will collide on these shared files.

## What Changes

Restructure from horizontal layers to vertical feature slices, keeping a `core/` module for shared traits/types and an `infra/` module for shared I/O adapters:

```
src/
├── main.rs
├── config.rs
├── lib.rs
│
├── domain/              ← shared traits + types (stable, rarely touched — unchanged)
│   ├── action/          ← ActionId, Version, CommitSha, etc.
│   ├── manifest.rs      ← Manifest type
│   ├── lock.rs          ← Lock type
│   ├── workflow.rs      ← WorkflowScanner/Updater traits, WorkflowError
│   ├── workflow_actions.rs ← WorkflowActionSet, LocatedAction
│   ├── resolution.rs    ← VersionRegistry trait, ActionResolver
│   ├── command.rs       ← Command + CommandReport traits (moved from command/traits.rs)
│   └── error.rs         ← AppError (moved from command/app.rs)
│
├── infra/               ← shared I/O adapters (stable, rarely touched)
│   ├── github.rs        ← GithubRegistry (moved from infrastructure/)
│   ├── workflow.rs      ← FileWorkflowScanner/Updater (moved from infrastructure/)
│   ├── manifest.rs      ← parse/write manifest TOML (moved from infrastructure/)
│   ├── lock.rs          ← parse/write lock file (moved from infrastructure/)
│   └── repo.rs          ← find_root (moved from infrastructure/)
│
├── tidy/                ← feature slice: owns plan + report + Command impl
│   ├── mod.rs           ← Command impl + plan/apply logic (from command/tidy.rs)
│   └── report.rs        ← TidyReport (extracted from output/report.rs)
│
├── init/                ← feature slice: owns Command impl + report
│   ├── mod.rs           ← Command impl (from command/init.rs)
│   └── report.rs        ← InitReport (extracted from output/report.rs)
│
├── upgrade/             ← feature slice: owns plan + report + Command impl
│   ├── mod.rs           ← Command impl + plan/apply logic (from command/upgrade.rs)
│   └── report.rs        ← UpgradeReport (extracted from output/report.rs)
│
├── lint/                ← feature slice: owns rules + report + Command impl
│   ├── mod.rs           ← Command impl + orchestration (from command/lint/mod.rs)
│   ├── rules/           ← Individual lint rules (from command/lint/*.rs)
│   └── report.rs        ← LintReport (extracted from output/report.rs)
│
└── output/              ← presentation (stays, but slimmed down)
    ├── lines.rs         ← OutputLine enum
    ├── printer.rs       ← Printer
    └── log_file.rs      ← LogFile
```

- **Kept**: `domain/` stays as `domain/` — it already has the right name.
- **Renamed**: `infrastructure/` → `infra/`. No logic changes.
- **Split**: `output/report.rs` splits — each report type moves into its feature's `report.rs`.
- **Split**: `domain/plan.rs` splits — `TidyPlan` moves to `tidy/`, `UpgradePlan` moves to `upgrade/`.
- **Move**: `command/common.rs` helpers move to `main.rs` (they're only used there).
- **Delete**: `command/` directory is removed entirely. Each command becomes a top-level feature module.

## Capabilities

### Modified Capabilities

- No spec-level behavior changes. This is a pure structural refactor — the same code, reorganized.

## Impact

- **`use` paths change**: `crate::infrastructure::*` → `crate::infra::*`, `crate::command::*` → `crate::{tidy,init,upgrade,lint}::*`. `crate::domain::*` stays unchanged.
- **`main.rs`**: Import paths update. `command::common` helpers inline here.
- **`config.rs`**: Import paths update (`domain` → `core`, `infrastructure` → `infra`).
- **Tests**: Import paths update. No logic changes.
- **No new dependencies**.
- **No behavior changes** — all public CLI behavior is identical.
