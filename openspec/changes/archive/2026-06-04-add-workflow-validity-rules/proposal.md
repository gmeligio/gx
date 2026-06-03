## Why

`gx lint` today catches **action hygiene** (unpinned/SHA-mismatched/stale `uses:`) and **workflow security** (the rule family added in `add-workflow-security-rules`). It does not catch **workflow validity** — structurally broken references that GitHub Actions will accept at parse time but fail or silently misbehave at run time.

The two highest-value validity checks, both called out as deferred scope in the `add-workflow-security-rules` proposal ("actionlint-style **correctness** rules … a separate scope"):

1. **Dangling `needs:` references** — a job lists `needs: [does-not-exist]`, or references a job that was renamed. GitHub fails the run with a confusing "job depends on unknown job" error only when the workflow is dispatched; `gx lint` should catch it statically.
2. **Invalid `${{ }}` context references to other jobs/steps** — `needs.<job>.outputs.<x>` where `<job>` isn't in this job's `needs:`, or `steps.<id>.outputs.<x>` where no step with that `id` exists earlier in the job. These resolve to empty strings at run time — the classic "my output is mysteriously blank" failure — with no error surfaced anywhere.

The motivating user is the flutter-docker-image maintainer, who already runs `gx lint` in CI and just refactored `update-version.yml` (the p12 symmetric-platform-updates change) — a restructure that renamed jobs (`update_android_version` → `update-android-version`), added a new `compose-version-manifest` job, and rewired `needs:` and `needs.<job>.outputs.*` references across five jobs. Exactly the change where a dangling reference or a typo'd `steps.<id>.outputs` is easy to introduce and invisible until a scheduled run misfires. They reached for the `actionlint` binary to verify it; this change brings that check into the tool they already run.

**Out of scope (deferred):**

- Full `${{ }}` expression *syntax* validation (operator grammar, function arity). This needs a real expression parser; the rules here do targeted reference-resolution scanning, not a grammar.
- Unknown runner labels (`runs-on: ubuntu-99.04`) — needs a runner-label catalog.
- Deprecated `set-env`/`set-output` workflow commands inside `run:` — belongs with shell analysis (see the sibling `add-run-shellcheck-rule` change).
- `shellcheck` on `run:` blocks — sibling change `add-run-shellcheck-rule`.

## What Changes

- Two new rules added to `gx lint`:

| Rule name | Default | What it catches |
|-----------|---------|-----------------|
| `dangling-reference` | error | A job's `needs:` lists a job id that does not exist in the workflow. |
| `invalid-expression` | error | A `${{ }}` reference to `needs.<job>.*` where `<job>` is not in the referencing job's `needs:` list, or `steps.<id>.*` where no earlier step in the same job declares that `id`. |

- **Domain-model extension.** The current `domain::workflow_parsed::{Job, Step}` is intentionally minimal (it captures only what the security rules need): `Job` has no `needs` and no `outputs`; `Step` has no `id`. This change adds:
  - `Job.needs: Vec<String>` (parsed from the `needs:` scalar-or-sequence form),
  - `Job.outputs: BTreeMap<String, String>` (so a future cross-job output-existence check has the data; the `invalid-expression` rule uses the keys),
  - `Step.id: Option<String>`.
- Both rules consume `Context.workflows_full` (the `&[ParsedWorkflow]` already threaded to the security rules). No new parse pass — the existing `serde_saphyr` parse is extended to populate the new fields.
- `RuleName` gains `DanglingReference` and `InvalidExpression` variants (kebab-case `dangling-reference`, `invalid-expression`), wired through `Display`/`FromStr` and the rule registry exactly like the security-rule family.
- The existing `[lint.rules]` config + `ignore = [...]` surface extends with no new syntax. `ignore` entries scope by `workflow`/`job` (the `action` key is meaningless here, consistent with the other workflow-scoped rules).
- README and `docs/lint-rules.md` gain a "Workflow validity" subsection; `docs/demo.tape` unchanged (no new command).

## Capabilities

### Modified Capabilities

- `lint-command`: gains two new rule requirements run by default. The capability's user-value statement ("users detect and fix problems in their GitHub Actions workflows using `gx lint`") expands from action drift + workflow security to also cover **structural validity** — references that resolve to nothing at run time.

### New Capabilities

_None._ Expansion of the existing `lint-command` capability, consistent with how `add-workflow-security-rules` added its family. Users discover all rules from one place.

## Spec gate

Required. Adds user-facing behavior: two new default-error rules that can fail CI for users who upgrade. Meets the relevance gate ("Adds, removes, or changes user-facing behavior").

## Impact

- **User-visible breaking change**: `gx lint` will produce new error-level diagnostics on workflows with dangling references or unresolved expressions that previously passed. Opt out per-rule via `level = "off"` in `gx.toml`. CHANGELOG calls this out.
- **Affected source**: new `src/lint/dangling_reference.rs` and `src/lint/invalid_expression.rs`; extension of `domain::workflow_parsed::{Job, Step}` and its `serde` wire structs; new `RuleName` variants; registry wiring in `src/lint/command.rs`.
- **Performance**: reuses the existing full-workflow parse; the added fields are cheap. Reference resolution is O(jobs + steps) per workflow.
- **Risk — false positives on dynamic expressions**: `${{ }}` can reference contexts the rule doesn't model (`env`, `vars`, `matrix`, `inputs`, `fromJSON(...)` indirection). The rule MUST only flag `needs.*` and `steps.*` references it can fully resolve, and MUST NOT flag a reference whose job/step segment is itself an expression or comes from `matrix`. The design walks through the conservative-matching rules.
- **Risk — `needs:` scalar vs sequence**: `needs: build` and `needs: [build, test]` are both valid YAML shapes. The wire parser must accept both. Covered by a deserialization test.
- **No new dependencies.**
