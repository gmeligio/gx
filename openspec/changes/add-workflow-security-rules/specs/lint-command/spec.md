## MODIFIED Requirements

### Requirement: Zero-config runs all rules at defaults

The system SHALL run all built-in rules at their hardcoded default levels when no `[lint.rules]` section is present in `gx.toml`. The default set covers both action-hygiene rules and workflow-security rules.

#### Scenario: No lint config

- **GIVEN** `gx.toml` has an `[actions]` section but no `[lint]` section
- **WHEN** user runs `gx lint`
- **THEN** all rules run at their default levels:
  - `sha-mismatch` = error
  - `unpinned` = error
  - `unsynced-manifest` = error
  - `stale-comment` = warn
  - `missing-permissions` = error
  - `excessive-permissions` = warn
  - `dangerous-trigger` = error
  - `pr-head-checkout` = error
  - `missing-concurrency` = warn
  - `unprotected-secrets` = error

### Requirement: Configure rule severity

The system SHALL allow each rule's severity to be set to `error`, `warn`, or `off` via the `[lint.rules]` section in `gx.toml`. Unrecognized rule names produce a parse error.

#### Scenario: All valid rule names accepted

- **GIVEN** `gx.toml` contains any combination of `sha-mismatch`, `unpinned`, `stale-comment`, `unsynced-manifest`, `missing-permissions`, `excessive-permissions`, `dangerous-trigger`, `pr-head-checkout`, `missing-concurrency`, `unprotected-secrets` in `[lint.rules]`
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL succeed and each rule's configured level is applied

#### Scenario: Disable a workflow-security rule

- **GIVEN** `gx.toml` contains `missing-concurrency = { level = "off" }`
- **WHEN** user runs `gx lint`
- **THEN** the `missing-concurrency` rule does not run and produces no diagnostics

## ADDED Requirements

### Requirement: missing-permissions rule

The system SHALL detect when a workflow file does not declare a top-level `permissions:` block. The user who benefits is a security-conscious maintainer who needs every workflow to declare an explicit, narrowed-down token scope; without this rule, an undeclared block defaults to the broad classic scope and the omission is invisible in PR review.

#### Scenario: Workflow declares top-level permissions

- **GIVEN** `ci.yml` has `permissions: { contents: read }` at the top level
- **WHEN** `missing-permissions` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Workflow has no permissions block

- **GIVEN** `legacy.yml` has no top-level `permissions:` block
- **WHEN** `missing-permissions` rule runs
- **THEN** an error diagnostic is produced identifying the file

---

### Requirement: excessive-permissions rule

The system SHALL detect when a workflow's top-level `permissions:` block declares any scope broader than `contents: read`. Broader scopes belong at job level so they are narrowed to the specific job that needs them. The user who benefits is the maintainer reviewing a Dependabot bump or a contributor's new workflow — a write scope at top level applies to every job, including jobs that should not have it.

#### Scenario: Top-level scope is read-only

- **GIVEN** a workflow with top-level `permissions: { contents: read }`
- **WHEN** `excessive-permissions` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Top-level scope is `write-all`

- **GIVEN** a workflow with top-level `permissions: write-all`
- **WHEN** `excessive-permissions` rule runs
- **THEN** a warn diagnostic is produced identifying the file and the offending scope

#### Scenario: Top-level scope includes a write permission

- **GIVEN** a workflow with top-level `permissions: { contents: write, packages: read }`
- **WHEN** `excessive-permissions` rule runs
- **THEN** a warn diagnostic is produced

#### Scenario: Job-level write scope is acceptable

- **GIVEN** a workflow with top-level `permissions: { contents: read }` and one job declaring `permissions: { packages: write }`
- **WHEN** `excessive-permissions` rule runs
- **THEN** no diagnostic is produced (job-level scopes are the correct location)

---

### Requirement: dangerous-trigger rule

The system SHALL detect when a workflow uses `on: pull_request_target:` or `on: workflow_run:`. Both triggers run in the target-repository context with full secrets and a write-scoped `GITHUB_TOKEN`, and both are reachable from fork PRs. The user who benefits is the maintainer protecting against the "pwn request" attack class — the precondition for the May 2026 TanStack-class supply-chain attack — and against the parallel `workflow_run` artifact-poisoning class.

#### Scenario: Workflow uses pull_request

- **GIVEN** a workflow with `on: pull_request:`
- **WHEN** `dangerous-trigger` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Workflow uses pull_request_target

- **GIVEN** a workflow with `on: pull_request_target:`
- **WHEN** `dangerous-trigger` rule runs
- **THEN** an error diagnostic is produced identifying the file and the `pull_request_target` trigger
- **AND** the diagnostic message includes a pointer to the pull_request alternative

#### Scenario: Workflow uses workflow_run

- **GIVEN** a workflow with `on: workflow_run:` consuming artifacts from another workflow
- **WHEN** `dangerous-trigger` rule runs
- **THEN** an error diagnostic is produced identifying the file and the `workflow_run` trigger
- **AND** the diagnostic message explains that `github.repository == ...` guards do not mitigate the risk

#### Scenario: Workflow uses both pull_request_target and workflow_run

- **GIVEN** a workflow whose `on:` map includes both triggers
- **WHEN** `dangerous-trigger` rule runs
- **THEN** two distinct error diagnostics are produced, one per trigger, so the user can address each line independently

#### Scenario: Reviewed dangerous-trigger workflow opts out

- **GIVEN** `gx.toml` contains `dangerous-trigger = { level = "error", ignore = [{ workflow = ".github/workflows/pr-comment-handler.yml" }] }`
- **WHEN** the named workflow uses `pull_request_target` or `workflow_run`
- **THEN** no diagnostic is produced for that file
- **AND** other workflows using either trigger still produce diagnostics

---

### Requirement: pr-head-checkout rule

The system SHALL detect when a workflow that has access to write-scoped tokens or secrets checks out the PR head ref. The user who benefits is the maintainer defending against the "checkout PR HEAD in privileged context" pattern — the specific code path the TanStack incident weaponized.

A workflow is **privileged** when any job declares a write scope in `permissions:` OR any step references `secrets.*`. A checkout is **of PR HEAD** when the step's `with.ref` (textually) contains `github.event.pull_request.head.sha`, `github.head_ref`, or `github.event.pull_request.head.ref`.

#### Scenario: Privileged workflow checks out PR HEAD

- **GIVEN** a workflow with `permissions: { contents: write }` at job level and a step `uses: actions/checkout@... with: { ref: ${{ github.event.pull_request.head.sha }} }`
- **WHEN** `pr-head-checkout` rule runs
- **THEN** an error diagnostic is produced identifying the file, job, and step

#### Scenario: Non-privileged workflow checks out PR HEAD

- **GIVEN** a read-only workflow (no write scopes, no secrets) that checks out `github.event.pull_request.head.sha`
- **WHEN** `pr-head-checkout` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Privileged workflow does not check out PR HEAD

- **GIVEN** a write-scoped workflow that checks out the default ref
- **WHEN** `pr-head-checkout` rule runs
- **THEN** no diagnostic is produced

---

### Requirement: missing-concurrency rule

The system SHALL detect when a workflow triggered by `push:` or `schedule:` does not declare a `concurrency:` block. The user who benefits is the maintainer who merges two PRs in quick succession — without concurrency control, two runs race for the same tag/commit/registry-push and the second silently fails or overwrites the first.

#### Scenario: Push-triggered workflow with concurrency

- **GIVEN** a workflow with `on: push:` and a top-level `concurrency:` block
- **WHEN** `missing-concurrency` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Push-triggered workflow without concurrency

- **GIVEN** a workflow with `on: push:` and no `concurrency:` block
- **WHEN** `missing-concurrency` rule runs
- **THEN** a warn diagnostic is produced

#### Scenario: PR-triggered workflow without concurrency

- **GIVEN** a workflow with `on: pull_request:` only and no `concurrency:` block
- **WHEN** `missing-concurrency` rule runs
- **THEN** no diagnostic is produced (rule applies only to push/schedule triggers)

---

### Requirement: unprotected-secrets rule

The system SHALL detect when a `pull_request`-triggered workflow references a user-managed secret (any `secrets.<NAME>` where `<NAME>` is not `GITHUB_TOKEN`) from a step that is not guarded by a fork-PR `if:` gate. The gate is satisfied when the effective `if:` (job-level AND step-level, concatenated) textually contains `github.event.pull_request.head.repo.full_name == github.repository` or the structurally-equivalent `github.repository_owner ==` shape.

`secrets.GITHUB_TOKEN` is excluded because GitHub automatically downgrades it to read-only on fork PRs regardless of the workflow's declared permissions. Workflows that have widened the token via top-level `permissions:` are caught by `excessive-permissions`.

The user who benefits is the maintainer enforcing "user-managed secrets never reach fork PR code" — the rule makes the gate explicit so PR review does not have to recompute it for every secret reference.

#### Scenario: PR-triggered workflow uses a user secret with the fork gate

- **GIVEN** a workflow with `on: pull_request:` and a step `if: github.event.pull_request.head.repo.full_name == github.repository ... password: ${{ secrets.DOCKER_HUB_TOKEN }}`
- **WHEN** `unprotected-secrets` rule runs
- **THEN** no diagnostic is produced

#### Scenario: PR-triggered workflow uses a user secret without the fork gate

- **GIVEN** a workflow with `on: pull_request:` and a step that references `secrets.DOCKER_HUB_TOKEN` with no `if:` guard
- **WHEN** `unprotected-secrets` rule runs
- **THEN** an error diagnostic is produced identifying the file, job, step, and the unguarded secret name

#### Scenario: PR-triggered workflow uses only `secrets.GITHUB_TOKEN`

- **GIVEN** a workflow with `on: pull_request:` and a step referencing `secrets.GITHUB_TOKEN` with no `if:` guard
- **WHEN** `unprotected-secrets` rule runs
- **THEN** no diagnostic is produced (GitHub auto-scopes `GITHUB_TOKEN` to read-only on fork PRs)

#### Scenario: Non-PR workflow uses a secret without the gate

- **GIVEN** a workflow with `on: push: { branches: [main] }` that references `secrets.DOCKER_HUB_TOKEN`
- **WHEN** `unprotected-secrets` rule runs
- **THEN** no diagnostic is produced (no fork-PR path can trigger this workflow)

#### Scenario: pull_request_target workflow uses a secret

- **GIVEN** a workflow with `on: pull_request_target:` that references a secret
- **WHEN** `unprotected-secrets` rule runs
- **THEN** no diagnostic is produced (`dangerous-trigger` covers this; double-reporting would be noise)

#### Scenario: workflow_run workflow uses a secret

- **GIVEN** a workflow with `on: workflow_run:` that references a secret
- **WHEN** `unprotected-secrets` rule runs
- **THEN** no diagnostic is produced (`dangerous-trigger` covers this; double-reporting would be noise)
