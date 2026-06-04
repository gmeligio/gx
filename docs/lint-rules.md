# Lint rules

`gx lint` reports several families of issues:

- **Action hygiene** — verify every workflow `uses:` reference is pinned, manifested, and consistently commented.
- **Workflow security** — flag patterns that expose secrets, the repo write token, or untrusted code execution to fork PRs and other adversarial inputs.
- **Workflow validity** — catch references that parse but fail or silently resolve to nothing at run time.
- **Shell analysis** — run `shellcheck` over `run:` shell bodies to surface shell bugs at lint time.

Every rule is identified by a kebab-case name and configured under `[lint.rules]` in `.github/gx.toml`:

```toml
[lint.rules]
unpinned = { level = "error" }
dangerous-trigger = { level = "off" }
unprotected-secrets = { level = "error", ignore = [{ workflow = ".github/workflows/release.yml" }] }
```

Levels are `error` (fail the run), `warn` (report but don't fail), or `off` (skip). Each rule has a built-in default level that applies when the rule is unconfigured.

The `ignore` list takes intersection semantics: every key you specify must match for the ignore to apply. For workflow-security rules the `action` key is meaningless — diagnostics are scoped to a workflow (and sometimes a job/step), not to an action reference. Omit `action` when ignoring a workflow-security finding; specifying it will cause the ignore not to match.

## Action-hygiene rules

### sha-mismatch *(default: error)*

The SHA pinned in a workflow does not match the SHA recorded in `gx.lock` for that action + specifier. Run `gx tidy` to repin, or update `gx.lock` if the workflow is correct.

### unpinned *(default: error)*

A `uses:` reference points at a tag, branch, or `@main`/`@master` instead of a 40-character commit SHA. Run `gx tidy` to pin.

### stale-comment *(default: warn)*

The `# v1.2.3` comment alongside a pinned SHA does not match the lock-resolved version. Run `gx tidy` to regenerate the comment.

### unsynced-manifest *(default: error)*

A `uses:` reference exists in a workflow but is missing from the manifest (`gx.toml`). Run `gx tidy` (or `gx init`) to add the action.

## Workflow-security rules

### missing-permissions *(default: error)*

The workflow has no top-level `permissions:` block, so it inherits the repo-default token scopes — usually broad. Add an explicit block, ideally starting from `permissions: {}` or `permissions: { contents: read }` and granting only what the workflow needs.

### excessive-permissions *(default: error)*

The top-level `permissions:` grants more than `contents: read`. `write-all` and `read-all` always trigger this rule; per-scope maps trigger when they grant any write scope or non-`contents` scope. Scope down to the minimum the workflow actually requires, or use job-level overrides.

### dangerous-trigger *(default: error)*

The workflow uses `pull_request_target` or `workflow_run`. Both run in the *target* repository context with full secret access and a write-scoped `GITHUB_TOKEN`, and both are reachable from fork PRs. Prefer `pull_request` unless you genuinely need a privileged trigger; if you do, gate every step that uses secrets or writes to the repo with a fork-PR check (`github.event.pull_request.head.repo.full_name == github.repository`). One diagnostic is emitted per dangerous trigger so the user can act on the right line.

### pr-head-checkout *(default: error)*

A privileged workflow (any job with write permissions OR any step referencing `secrets.*`) checks out the PR HEAD ref — `github.event.pull_request.head.sha`, `github.event.pull_request.head.ref`, or `github.head_ref`. This executes untrusted code with privileged context. Either drop the privileged context, drop the HEAD checkout, or gate the privileged step with the fork-PR check above.

### missing-concurrency *(default: warn)*

The workflow triggers on `push` or `schedule` but has no top-level `concurrency:` block, so overlapping runs are not cancelled. Add `concurrency: { group: "${{ github.workflow }}-${{ github.ref }}", cancel-in-progress: true }` or similar to reclaim runner time.

### unprotected-secrets *(default: error)*

A `pull_request` workflow references a user-managed secret (anything except `GITHUB_TOKEN`) in a step that lacks the canonical fork-PR gate. `secrets.GITHUB_TOKEN` is exempt because GitHub auto-scopes it down on fork PRs. The accepted gates are:

- `github.event.pull_request.head.repo.full_name == github.repository`
- `github.repository_owner == ...`

A job-level `if:` propagates to its steps. Workflows that already use `pull_request_target` or `workflow_run` are skipped by this rule (the broader `dangerous-trigger` covers them).

## Workflow-validity rules

These rules catch references that GitHub Actions accepts at parse time but that fail or silently resolve to nothing at run time — the kind of break you would otherwise discover only when a scheduled or dispatched run misfires (a blank output, or an "unknown job" error far from the edit that caused it). They are scoped per workflow/job; the `action` key in an `ignore` entry is meaningless for them.

### dangling-reference *(default: error)*

A job's `needs:` lists a job id that does not exist in the workflow — usually a typo (`needs: [buld]`) or a job that was renamed without updating its dependents. Both the scalar form (`needs: build`) and the sequence form (`needs: [build, test]`) are accepted. GitHub fails the run with "job depends on unknown job" only when the workflow is dispatched; this catches it at lint time.

### invalid-expression *(default: error)*

A `${{ }}` reference to `needs.<job>` or `steps.<id>` that cannot resolve:

- `needs.<job>.…` where `<job>` is not in the referencing job's `needs:` list (including when the job declares no `needs:` at all).
- `needs.<job>.outputs.<key>` where `<job>` is a declared dependency that exposes a non-empty inline `outputs:` map and `<key>` is not one of its keys. When the producing job has no inline `outputs:` map — for example a job that `uses:` a reusable workflow, whose outputs are defined in the called file — the output key is not checked.
- `steps.<id>.…` where no *earlier* step in the same job declares `id: <id>` (you can't read an output before the step runs).

The rule only flags references it can fully resolve to a bare identifier. Dynamic references whose job/step segment is indexed (`needs[matrix.target]`) or built by a function (`steps[format(...)]`) are skipped, as are out-of-scope contexts (`env`, `vars`, `matrix`, `inputs`, `github`, `secrets`, `runner`, `job`). Step *output keys* (`steps.<id>.outputs.<key>`) are intentionally not validated — what a step produces is not knowable from the workflow file.

## Shell-analysis rules

### run-shellcheck *(default: warn)*

Runs the [`shellcheck`](https://www.shellcheck.net/) static analyzer over the shell body of each `run:` step and reports each finding as a diagnostic scoped to the workflow, job, and step (the message carries the `SCxxxx` code, severity, and the in-script line). This brings the same coverage [actionlint](https://github.com/rhysd/actionlint) gives — unquoted expansions that word-split, masked pipeline failures, and ~250 other checks — into `gx lint`, without configuring a separate tool.

**Which steps are analyzed.** Only steps whose *effective shell* is `bash` or `sh`. The effective shell is resolved in precedence order: the step's `shell:`, then the job's `defaults.run.shell`, then the workflow's `defaults.run.shell`, then a default of `bash`. Steps that resolve to a non-POSIX shell (`pwsh`, `python`, `cmd`, ...) are skipped.

**`${{ }}` expressions.** Before analysis, GitHub Actions `${{ }}` expressions are neutralized (replaced with equal-length underscores so columns are preserved), and the `shellcheck` codes that this substitution would otherwise trip are excluded (`SC1091`, `SC2050`, `SC2153`, `SC2154`, `SC2157`, `SC2194`, `SC2043`). This mirrors actionlint and prevents false positives on workflows that interpolate expressions into `run:` blocks.

**Optional dependency.** `shellcheck` must be on `PATH`. When it is not found, the rule does **not** fail the lint run: it emits a single informational diagnostic noting that it was skipped, and the exit code is unaffected. Install `shellcheck` (e.g. via your package manager, or `mise`) to activate the rule. Because it defaults to `warn`, set `run-shellcheck = { level = "error" }` to make findings fail CI.

This rule is scoped per workflow/job — like the other workflow rules, the `action` key in an `ignore` entry is meaningless for it.

## Disabling rules

To turn off a rule entirely:

```toml
[lint.rules]
missing-concurrency = { level = "off" }
```

To keep a rule active but skip a specific workflow:

```toml
[lint.rules]
dangerous-trigger = { level = "error", ignore = [
    { workflow = ".github/workflows/release.yml" },
] }
```

To skip a single job within a workflow:

```toml
[lint.rules]
unprotected-secrets = { level = "error", ignore = [
    { workflow = ".github/workflows/ci.yml", job = "publish" },
] }
```
