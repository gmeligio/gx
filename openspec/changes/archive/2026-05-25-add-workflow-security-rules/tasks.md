## 1. Expand the domain to expose workflow metadata

- [x] 1.1 Add `domain::workflow::Parsed` with `on`, `permissions`, `concurrency`, `jobs` (each job with `permissions`, `if`, steps).
- [x] 1.2 Add `domain::workflow::Trigger` enum covering `pull_request`, `pull_request_target`, `push`, `schedule`, `workflow_dispatch`, `workflow_call`, `workflow_run`, `release`, `tags`, `other(String)`. Multi-trigger workflows hold a `Vec<Trigger>`.
- [x] 1.3 Update `infra::workflow_scan::FileScanner` to parse the full YAML once via `serde_saphyr` (already in use at `scanner.rs:192`) and emit both `Parsed` and the existing `Located` action list. Verify existing tests still pass and add a regression test confirming the single parse yields the same `WorkflowAction` list the prior two-pass scan produced.
- [x] 1.4 Verify: `mise run build` and `mise run test` pass.

## 2. Extend the lint engine

- [x] 2.1 Add `workflows_full: &'ctx [Parsed]` field to `Context`. Existing rules continue to use `workflows`.
- [x] 2.2 Add six variants to `RuleName` enum: `MissingPermissions`, `ExcessivePermissions`, `DangerousTrigger`, `PrHeadCheckout`, `MissingConcurrency`, `UnprotectedSecrets`. Update `FromStr`, `Display`, and the kebab-case serde rename.
- [x] 2.3 Update `command.rs` lint-command runner to instantiate the six new rules and pass `workflows_full` in the context.
- [x] 2.4 Update `Diagnostic` to optionally carry job/step location (some rules emit step-scoped diagnostics).
- [x] 2.5 Verify: `mise run build` passes.

## 3. Implement the six rules

- [x] 3.1 `src/lint/missing_permissions.rs` — error if `Parsed.permissions` is `None`. Unit tests: clean workflow, missing block, top-level present.
- [x] 3.2 `src/lint/excessive_permissions.rs` — warn if top-level `permissions:` declares anything other than `contents: read`. Unit tests: `contents: read` only (clean), `contents: write` at top level (warns), `permissions: write-all` (warns), `permissions: read-all` (warns — broader than `contents: read`).
- [x] 3.3 `src/lint/dangerous_trigger.rs` — error if `Parsed.on` contains `pull_request_target` OR `workflow_run`. Diagnostic message names the specific trigger so the user can act on the right line. Unit tests: pull_request only (clean), pull_request_target (errors), workflow_run (errors), both pull_request_target + workflow_run in same file (two diagnostics), workflow_run with ignore opt-out (clean).
- [x] 3.4 `src/lint/pr_head_checkout.rs` — error if (any job has a write permission OR any step references `secrets.*`) AND (any step has `with.ref` matching `github.event.pull_request.head.sha` / `github.head_ref` / `github.event.pull_request.head.ref`). Unit tests: privileged + PR-head ref (errors), non-privileged + PR-head ref (clean), privileged without ref (clean).
- [x] 3.5 `src/lint/missing_concurrency.rs` — warn if `Parsed.on` contains `push` or `schedule` and `Parsed.concurrency` is `None`. Unit tests: push without concurrency (warns), push with concurrency (clean), pull_request without concurrency (clean).
- [x] 3.6 `src/lint/unprotected_secrets.rs` — error per the design.md algorithm. `secrets.GITHUB_TOKEN` is excluded from the matched set (auto-scoped down on fork PRs by GitHub). Unit tests: PR workflow + user secret + no gate (errors), PR workflow + user secret + correct gate (clean), PR workflow + `secrets.GITHUB_TOKEN` + no gate (clean — excluded), pull_request_target + secret (clean — `dangerous-trigger` covers this), workflow_run + secret (clean — `dangerous-trigger` covers this), non-PR workflow + secret (clean), PR workflow + user secret + custom non-canonical gate (errors — opt out via `ignore`).
- [x] 3.7 Verify: `mise run test` passes; each rule has the four standard test shapes (clean, fails, ignore-scopes, disabled).

  - Clean and fails shapes live in each rule's unit tests.
  - Ignore-scopes and disabled shapes live at the runner level (see §4 integration tests):
    those concerns are implemented by `matches_ignore_workflow` and the `Level::Off`
    short-circuit in `command.rs`, not by the rule's `check()` function.

## 4. Configuration integration

- [x] 4.1 Confirm the existing `[lint.rules]` TOML parser accepts the six new rule names (it parses `RuleName` via serde — adding enum variants is enough). Verified via `lint_config_parses_all_six_new_rule_names`.
- [x] 4.2 Confirm the existing `ignore = [{ action = ..., workflow = ..., job = ... }]` mechanism works for workflow-scoped rules. The `action` key is meaningless for `missing-permissions` and similar — `matches_ignore_workflow` short-circuits if `action` is set so users should omit it; documented in `docs/lint-rules.md` (§6.3).
- [x] 4.3 Add integration tests that exercise `level = "off"` on each new rule. Done in `tests/integ_lint.rs` via the `*_can_be_disabled` set.

## 5. Output and CLI

- [x] 5.1 `gx lint --help` now enumerates the two rule families and points to `docs/lint-rules.md`.
- [x] 5.2 Summary line already counts uniformly by `Level` (see `Report::render` and existing `render_lint_with_violations` test). No change required — the format is rule-category-agnostic.
- [x] 5.3 Stable diagnostic ordering by `(workflow_path, job_id, step_index, rule_name)` enforced by `collect_diagnostics` and asserted via `diagnostics_are_stably_sorted_across_workflows_jobs_and_rules`.

## 6. Documentation

- [x] 6.1 README's "Commands" table now mentions workflow-security and links to `docs/lint-rules.md`.
- [x] 6.2 Skipped — `docs/demo.tape` contains no `lint` invocations (verified via grep).
- [x] 6.3 `docs/lint-rules.md` written: per-rule paragraph (what it catches, the canonical fix, default level), the two-family overview, and ignore semantics including the workflow-security caveat about omitting `action`.

## 7. Release

- [x] 7.1 CHANGELOG entry — handled by release-plz post-merge. release-plz (`.github/workflows/release-plz.yml`) reads the `feat(lint): ...` conventional commits on main and generates the `### Added` / `### Breaking` sections automatically when it opens the "chore: release vX.Y.Z" PR. The manual entry initially added in this PR was reverted.
- [x] 7.2 `Cargo.toml` version bump — also handled by release-plz post-merge (it computes the minor bump from the `feat` commits). The manual 0.7.1 → 0.8.0 bump initially added in this PR was reverted.
- [x] 7.3 Verified: `mise run test` (339 passing) and `mise run clippy` (exit 0; the project's lint task is `clippy`, not `lint`) succeed, `cargo build --release` succeeds.

## 8. Downstream coordination

- [ ] 8.1 After release, open a PR on `gmeligio/flutter-docker-image` that bumps the pinned gx version and configures the six rules at their default levels in `.github/gx.toml`. The PR's CI run validates that the rules catch the real workflow corpus.
