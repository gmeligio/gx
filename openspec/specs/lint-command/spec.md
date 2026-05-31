## Lint Command

Users detect and fix problems in their GitHub Actions workflows using `gx lint`.

---

### Requirement: Run lint checks with gx lint

The system SHALL provide a `gx lint` subcommand that validates workflows against the manifest and lock file without modifying any files.

#### Scenario: Clean repo with no issues
- **WHEN** user runs `gx lint` and all workflows are in sync with manifest and lock
- **THEN** the command exits with code 0

#### Scenario: Errors detected
- **WHEN** user runs `gx lint` and one or more rules produce error-level diagnostics
- **THEN** the command prints all diagnostics and exits with code 1

#### Scenario: Warnings only
- **WHEN** user runs `gx lint` and rules produce only warn-level diagnostics (no errors)
- **THEN** the command prints all diagnostics and exits with code 0

#### Scenario: No manifest file exists
- **WHEN** user runs `gx lint` and `gx.toml` does not exist
- **THEN** the command exits with code 0 (nothing to lint)

---

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
  - `excessive-permissions` = error
  - `dangerous-trigger` = error
  - `pr-head-checkout` = error
  - `missing-concurrency` = warn
  - `unprotected-secrets` = error

---

### Requirement: Configure rule severity

The system SHALL allow each rule's severity to be set to `error`, `warn`, or `off` via the `[lint.rules]` section in `gx.toml`. Unrecognized rule names produce a parse error.

#### Scenario: Unrecognized rule name in config
- **GIVEN** `gx.toml` contains `sha-missmatch = { level = "error" }` (typo)
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL fail with an error identifying the unrecognized rule name

#### Scenario: All valid rule names accepted
- **GIVEN** `gx.toml` contains any combination of `sha-mismatch`, `unpinned`, `stale-comment`, `unsynced-manifest`, `missing-permissions`, `excessive-permissions`, `dangerous-trigger`, `pr-head-checkout`, `missing-concurrency`, `unprotected-secrets` in `[lint.rules]`
- **WHEN** the manifest is parsed
- **THEN** parsing SHALL succeed and each rule's configured level is applied

#### Scenario: Disable a rule
- **GIVEN** `gx.toml` contains `stale-comment = { level = "off" }`
- **WHEN** user runs `gx lint`
- **THEN** the `stale-comment` rule does not run and produces no diagnostics

#### Scenario: Disable a workflow-security rule

- **GIVEN** `gx.toml` contains `missing-concurrency = { level = "off" }`
- **WHEN** user runs `gx lint`
- **THEN** the `missing-concurrency` rule does not run and produces no diagnostics

#### Scenario: Promote a rule to error
- **GIVEN** `gx.toml` contains `stale-comment = { level = "error" }`
- **WHEN** user runs `gx lint` and stale comments exist
- **THEN** stale comment diagnostics are reported as errors and the command exits with code 1

---

### Requirement: Ignore targets for rules

The system SHALL support ignore entries in rule configuration using typed keys: `action`, `workflow`, and `job`. Multiple keys in a single entry compose as intersection (narrowing scope).

#### Scenario: Ignore a specific action
- **GIVEN** `unpinned` rule has `ignore = [{ action = "actions/internal-tool" }]`
- **WHEN** user runs `gx lint` and `actions/internal-tool` is unpinned in a workflow
- **THEN** no diagnostic is produced for `actions/internal-tool`
- **AND** other unpinned actions still produce diagnostics

#### Scenario: Ignore scoped to workflow and job
- **GIVEN** `sha-mismatch` rule has `ignore = [{ action = "actions/checkout", workflow = ".github/workflows/legacy.yml", job = "compat" }]`
- **WHEN** `actions/checkout` has a SHA mismatch in `legacy.yml` job `compat`
- **THEN** no diagnostic is produced for that specific location
- **WHEN** `actions/checkout` has a SHA mismatch in `legacy.yml` job `build`
- **THEN** a diagnostic IS produced (different job, not covered by ignore)

#### Scenario: Ignore scoped to workflow only
- **GIVEN** `unpinned` rule has `ignore = [{ workflow = ".github/workflows/experimental.yml" }]`
- **WHEN** any action is unpinned in `experimental.yml`
- **THEN** no diagnostic is produced for actions in that workflow

---

## Lint Rules

### Requirement: sha-mismatch rule

The system SHALL detect when a workflow file references a SHA that differs from what the lock file specifies for that action and version.

#### Scenario: Workflow SHA matches lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4` and `gx.lock` maps `actions/checkout` v4 to `abc123`
- **WHEN** `sha-mismatch` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Workflow SHA differs from lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4` and `gx.lock` maps `actions/checkout` v4 to `def456`
- **WHEN** `sha-mismatch` rule runs
- **THEN** an error diagnostic is produced identifying the file, action, expected SHA, and actual SHA

---

### Requirement: unpinned rule

The system SHALL detect when a workflow file references an action using a tag (e.g., `@v4`) instead of a SHA-pinned reference (e.g., `@abc123 # v4`).

#### Scenario: Action is SHA-pinned
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4`
- **WHEN** `unpinned` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Action uses tag reference
- **GIVEN** `ci.yml` has `actions/checkout@v4`
- **WHEN** `unpinned` rule runs
- **THEN** an error diagnostic is produced identifying the file and action

---

### Requirement: unsynced-manifest rule

The system SHALL detect when the set of actions in workflows does not match the set of actions in the manifest.

#### Scenario: Action in workflow but not in manifest
- **GIVEN** `ci.yml` uses `actions/cache` but `gx.toml` does not list `actions/cache`
- **WHEN** `unsynced-manifest` rule runs
- **THEN** an error diagnostic is produced: action found in workflow but missing from manifest

#### Scenario: Action in manifest but not in any workflow
- **GIVEN** `gx.toml` lists `actions/setup-go` but no workflow file uses it
- **WHEN** `unsynced-manifest` rule runs
- **THEN** an error diagnostic is produced: action in manifest but unused in workflows

#### Scenario: Manifest and workflows are in sync
- **GIVEN** every action in `gx.toml` appears in at least one workflow and vice versa
- **WHEN** `unsynced-manifest` rule runs
- **THEN** no diagnostic is produced

---

### Requirement: stale-comment rule

The system SHALL detect when a version comment in a workflow file does not match the version that the lock file associates with that SHA.

#### Scenario: Comment matches lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v4` and `gx.lock` confirms `abc123` resolves to `v4`
- **WHEN** `stale-comment` rule runs
- **THEN** no diagnostic is produced

#### Scenario: Comment does not match lock
- **GIVEN** `ci.yml` has `actions/checkout@abc123 # v3` but `gx.lock` maps `abc123` to `v4`
- **WHEN** `stale-comment` rule runs
- **THEN** a warn diagnostic is produced identifying the file, action, stated version, and actual version

#### Scenario: No comment present
- **GIVEN** `ci.yml` has `actions/checkout@abc123` (no version comment)
- **WHEN** `stale-comment` rule runs
- **THEN** no diagnostic is produced (nothing to validate)

---

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
- **THEN** an error diagnostic is produced identifying the file and the offending scope

#### Scenario: Top-level scope includes a write permission

- **GIVEN** a workflow with top-level `permissions: { contents: write, packages: read }`
- **WHEN** `excessive-permissions` rule runs
- **THEN** an error diagnostic is produced

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

---

## Lint Output

### Requirement: Clean diagnostic output

The system SHALL print lint diagnostics as the primary output with clear severity prefixes and a summary line.

#### Scenario: Lint violations produce clean output
- **WHEN** user runs `gx lint` and violations are found
- **THEN** diagnostics are printed with `[error]` or `[warn]` prefixes
- **AND** a summary line shows total count, error count, and warning count
- **AND** the process exits with code 1
- **AND** no internal error wrapper message is printed

#### Scenario: Lint I/O error produces error message
- **WHEN** user runs `gx lint` and a workflow file cannot be read
- **THEN** an error message describing the I/O failure is printed
- **AND** the process exits with a non-zero code

#### Scenario: Mixed severity output
- **GIVEN** a workflow that triggers both error and warn diagnostics
- **WHEN** user runs `gx lint`
- **THEN** both are reported and exit code is 1 (due to errors)

#### Scenario: Warning-only produces exit code 0
- **GIVEN** a workflow that triggers only warn diagnostics (all error rules disabled)
- **WHEN** user runs `gx lint`
- **THEN** warnings are reported and exit code is 0

