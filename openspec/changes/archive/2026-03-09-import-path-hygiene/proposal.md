## Why

The codebase mixes `super::`, `crate::`, and `self::` import styles without a consistent rule. This makes it harder to read imports and reason about module relationships. There is no clippy lint for this ([rust-clippy#12796](https://github.com/rust-lang/rust-clippy/issues/12796) is still open), so we enforce it ourselves via `code_health.rs`.

## Rules

Three rules, ordered from simplest to most nuanced:

| # | Rule | Rationale |
|---|------|-----------|
| 1 | **Ban `super::super::`** | If you need to go two+ levels up, use `crate::` — it's clearer and survives refactoring |
| 2 | **Ban `crate::` when `super::` suffices** | Don't write `crate::domain::action::spec::X` from `domain/action/identity.rs` — that's just `super::spec::X` |
| 3 | **Ban `self::`** | Redundant — `use self::foo` is identical to `use foo` |

### Rule 2 in detail

A `crate::` import is replaceable by `super::` when the target is a sibling or child of the parent module. Given a file at depth `src/<a>/<b>/<c>.rs`, the parent module is `<a>/<b>/`. Anything under `<a>/<b>/` is reachable via a single `super::`, so a `crate::<a>::<b>::*` import from that file is a violation.

Example from `domain/action/identity.rs`:
- `use super::spec::Specifier` — **OK** (sibling via one `super::`)
- `use crate::domain::action::spec::Specifier` — **Violation** (same target, should use `super::`)
- `use crate::domain::lock::Lock` — **OK** (different subtree, `super::` would need two hops)

## What Changes

- **`tests/code_health.rs`**: Add a single test `import_path_hygiene` that scans all `.rs` files in `src/` and enforces the three rules above.

## Capabilities

### New Capabilities

- **Import path hygiene spec** (`openspec/specs/architecture-guardrails/spec.md`): Append the three import path rules to the existing architecture guardrails spec.

### Modified Capabilities

_(None — the existing code health tests are unchanged.)_

## Impact

- **`tests/code_health.rs`**: One new test function added.
- **`openspec/specs/architecture-guardrails/spec.md`**: Append import path hygiene requirements.
- **Existing source files**: Any current violations must be fixed to pass the new test. Violations are likely few — the codebase already mostly follows these conventions.
- **No user-facing changes.**

## Dependencies

- Independent of other changes.
