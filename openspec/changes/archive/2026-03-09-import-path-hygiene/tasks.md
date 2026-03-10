## Tasks

- [x] **1. Add `import_path_hygiene` test to `tests/code_health.rs`**

  Added `module_parent_prefix` helper and `import_path_hygiene` test enforcing all three rules. Rule 2 is scoped to file-level (non-indented) `use` statements and skips `tests.rs` files to avoid false positives from files included at non-standard module depths.

- [x] **2. Fix rule 1 violations — ban `super::super::`**

  Replaced all `super::super::` occurrences with `crate::` equivalents in:
  - `src/infra/manifest/patch.rs` (test module)
  - `src/infra/github/tests.rs`
  - `src/lint/stale_comment.rs` (test module, 3 occurrences — path expressions, not use lines)
  - `src/infra/lock/convert.rs` (test module)
  - `src/infra/manifest/convert.rs` (test module)

- [x] **3. Fix rule 2 violations — ban `crate::` when `super::` suffices**

  Replaced `crate::<parent>::` with `super::` in files where the target is one hop away:
  - `src/domain/plan.rs` (test module)
  - `src/domain/event.rs` (test module)
  - `src/domain/resolution.rs` (module level + test module)
  - `src/domain/lock/mod.rs` (module level + test module)
  - `src/domain/manifest/mod.rs` (module level + test module)
  - `src/domain/workflow.rs` (module level — unlisted but same violation)
  - `src/lint/report.rs` (module level — caught by test)

- [x] **4. Fix rule 3 violations — ban `self::`**

  Removed `self::` prefix from all `use self::` and `pub use self::` statements in:
  - `src/init/mod.rs`
  - `src/upgrade/mod.rs`
  - `src/tidy/mod.rs`
  - `src/lint/mod.rs`

- [x] **5. Append import path hygiene requirements to architecture guardrails spec**

  Added the three import path rules with requirement and scenarios to `openspec/specs/architecture-guardrails/spec.md`.

- [x] **6. Run tests to verify**

  `cargo test --test code_health` — 6 passed, 0 failed.
