## Context

The `src/domain/` module has three structural inconsistencies:

1. **`domain/lock/`** is a two-file directory (`mod.rs` + `tests.rs`) for a single concept. Rust resolves `lock.rs` identically to `lock/mod.rs`, so the directory adds no value.
2. **`domain/workflow_action.rs`** is a 16-line file containing one struct (`WorkflowAction`) that is consumed by `workflow_actions.rs` and a few downstream files. The singular/plural split is confusing.
3. **`domain/plan.rs`** contains `ManifestDiff`, `LockDiff`, and `WorkflowPatch` — all diff/patch types. The name "plan" conflicts with `upgrade::Plan` (an actual plan) and misleads readers.

## Goals / Non-Goals

**Goals:**
- Reduce directory nesting where it adds no organizational value
- Colocate tightly-coupled types into the same file
- Align file names with their contents

**Non-Goals:**
- Refactoring `domain/manifest/` (multi-concept module, tracked separately in `infra-mod-colocation`)
- Changing any public API behavior
- Restructuring modules outside `src/domain/`

## Decisions

### 1. Inline tests when flattening `lock/`

Merge `lock/tests.rs` into `lock.rs` as an inline `#[cfg(test)] mod tests` block rather than keeping a `#[path]` attribute.

**Rationale:** Inline test modules are the idiomatic Rust pattern for unit tests. The `#[path]` pattern was only used because the directory structure existed. The combined file (~411 lines: 136 logic + 275 tests) is within the project's 550-line total budget.

**Alternative considered:** Keep `lock/` as a directory. Rejected — no second file justifies the directory.

### 2. Move `WorkflowAction` to top of `workflow_actions.rs`

Place the struct definition before the `ActionSet` type that consumes it.

**Rationale:** `WorkflowAction` is a leaf type with no logic — it's a data struct. Placing it at the top of its consumer file follows the pattern of defining types before their aggregates. The file grows from 342 → ~355 lines (well within budget).

**Alternative considered:** Create a `workflow/` directory with both files. Rejected — overkill for two tightly-coupled types.

### 3. Update all import paths mechanically

For the `plan→diff` rename: update all `use crate::domain::plan::` to `use crate::domain::diff::` (10 files; `infra/workflow_update.rs` has two import sites — module-level and inside `#[cfg(test)]`).
For the `workflow_action` merge: update all `use crate::domain::workflow_action::` to `use crate::domain::workflow_actions::` (4 files).

**Rationale:** Rust's compiler will catch any missed import at build time. No runtime risk.

### 4. Land changes in sequence: rename → merge → flatten

**Rationale:** Each step is independently reviewable and revertible. The rename has the widest blast radius (8 files) but is purely mechanical. The flatten is the largest change but self-contained.

## Risks / Trade-offs

- **Wide import update for `plan→diff`** (10 files) → Mitigation: mechanical find-and-replace, compiler verifies completeness.
- **`lock.rs` at 411 lines** is the largest single domain file → Mitigation: 275 of those lines are tests. 136 logic lines is well under the 300-line target.
- **Git blame disruption** for flattened `lock.rs` → Mitigation: use `git log --follow` for history. Unavoidable cost of restructuring.
