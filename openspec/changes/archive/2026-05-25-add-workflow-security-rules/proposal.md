## Why

`gx lint` today catches **action-level** problems: unpinned `uses:` refs, SHA mismatches against the lock, stale version comments, manifest drift. It does not catch **workflow-level** security problems — the class of issues that lead to GitHub Actions supply-chain compromises (May 2026 TanStack npm incident, tj-actions/changed-files March 2025, etc.).

Repos that adopt gx for pinning end up reaching for a second tool — typically zizmor or actionlint — to cover the workflow-level checks. That fragments the user's CI:

- Two binaries to install and version-pin
- Two configuration surfaces (`gx.toml` and `.zizmor.yml`)
- Two failure modes in CI logs to reconcile

The flutter-docker-image repo (which already runs `gx lint` in CI) is the motivating user: they want one tool to enforce both "every action is SHA-pinned" and "no privileged workflow checks out PR HEAD." Today they have to either copy a zizmor workflow into their repo or hand-grep for fork-secret gates in PR review.

This change extends `gx lint` with workflow-level security rules so a single `gx lint` invocation covers both action hygiene and workflow security.

**Out of scope (deferred to a future change):**

- actionlint-style **correctness** rules (`${{ }}` expression syntax, unknown runner labels, deprecated `set-env`/`set-output`). These need an expression parser and runner-label catalog — a separate scope.
- `template-injection` rule (interpolating untrusted context into `run:` scripts). Requires shell-script analysis; deferrable.
- shellcheck integration.

## What Changes

- Six new rules added to `gx lint`, each with a sensible default severity:

| Rule name | Default | What it catches |
|-----------|---------|-----------------|
| `missing-permissions` | error | Workflow file has no top-level `permissions:` block |
| `excessive-permissions` | warn | Top-level `permissions:` declares anything broader than `contents: read` (broader scopes belong at job level) |
| `dangerous-trigger` | error | Workflow uses `on: pull_request_target:` or `on: workflow_run:` (both run in target-repo context with secrets and are triggerable by fork PRs) |
| `pr-head-checkout` | error | Workflow with secrets/write token checks out `${{ github.event.pull_request.head.sha }}` or `github.head_ref` |
| `missing-concurrency` | warn | Workflow triggered by `push:` or `schedule:` without a `concurrency:` block |
| `unprotected-secrets` | error | `pull_request`-triggered workflow references a non-`GITHUB_TOKEN` secret in a step without an `if:` guard on `github.event.pull_request.head.repo.full_name == github.repository`. `secrets.GITHUB_TOKEN` is excluded because GitHub auto-scopes it down on fork PRs. |

- `Context` (the structure passed to each `Rule::check`) gains a `workflows_full: &[ParsedWorkflow]` field exposing parsed workflow metadata: triggers, top-level permissions, concurrency, jobs (with their permissions/if/steps).
- A new `domain::workflow::Parsed` type encapsulates the YAML parse result. The existing action-scanner reuses this parse instead of re-parsing the file.
- The existing `[lint.rules]` configuration surface and `ignore = [...]` mechanism extend naturally to the new rules — no new TOML syntax.
- README and `docs/demo.tape` updated to mention the security-rule family.

## Capabilities

### Modified Capabilities

- `lint-command`: gains six new rule requirements and the contract that `gx lint` runs them by default. The capability's user-value statement ("users detect and fix problems in their GitHub Actions workflows using `gx lint`") expands from "problems" meaning action drift to "problems" meaning action drift + workflow-level security issues.

### New Capabilities

_None._ This is an expansion of the existing `lint-command` capability, not a new family. Adding a separate capability would fragment the user-facing surface — users discover all rules from one place (`gx lint --help` and the lint-command spec).

## Spec gate

Required. This change adds user-facing behavior (six new rules; new default error-level diagnostics that can fail CI for users who upgrade). Meets the relevance gate ("Adds, removes, or changes user-facing behavior").

## Impact

- **User-visible breaking change**: `gx lint` will produce new error-level diagnostics on workflows that previously passed. Users upgrading from the prior version SHALL be able to set any new rule to `level = "off"` in `gx.toml` to opt out. The CHANGELOG entry calls this out.
- **Affected source**: new files under `src/lint/` (one per rule), expansion of `domain::workflow` to expose parsed workflow metadata, extension of `Context` and `RuleName`.
- **Performance**: full YAML parse of every workflow (today gx scans only for `uses:` lines). For typical repos (<20 workflows × <500 lines) this is sub-50ms.
- **Risk**: false positives on `unprotected-secrets` and `pr-head-checkout` — the rule has to model "is this workflow triggered by `pull_request`?" and "is the secret access guarded?" The design.md walks through the false-positive analysis.
- **Risk**: `dangerous-trigger` firing on `workflow_run` will flag the canonical "two-workflow `pull_request` + `workflow_run`" mitigation pattern recommended by GitHub Security Lab. This is intentional — even non-checkout `workflow_run` jobs run with secrets and are exploitable via attacker-controlled artifacts. Users who have audited the consumer can opt out per-workflow via `ignore`.
- **Risk**: `excessive-permissions` is opinionated — some users keep top-level `contents: write` intentionally. Default-warn (not error) and configurable; this matches its informational nature.
- **Documentation**: each new rule gets a one-paragraph entry in the lint-command docs (DeepWiki regenerates from source).
- **No new dependencies**: `serde_saphyr` is already the YAML parser used by the action scanner (`src/infra/workflow_scan/scanner.rs`); the full-workflow parser reuses that same parse pass rather than parsing each file twice.
