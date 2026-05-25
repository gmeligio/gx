## 1. Expand the domain to expose workflow metadata

- [ ] 1.1 Add `domain::workflow::Parsed` with `on`, `permissions`, `concurrency`, `jobs` (each job with `permissions`, `if`, steps).
- [ ] 1.2 Add `domain::workflow::Trigger` enum covering `pull_request`, `pull_request_target`, `push`, `schedule`, `workflow_dispatch`, `workflow_call`, `workflow_run`, `release`, `tags`, `other(String)`. Multi-trigger workflows hold a `Vec<Trigger>`.
- [ ] 1.3 Update `infra::workflow_scan::FileScanner` to parse the full YAML once via `serde_saphyr` (already in use at `scanner.rs:192`) and emit both `Parsed` and the existing `Located` action list. Verify existing tests still pass and add a regression test confirming the single parse yields the same `WorkflowAction` list the prior two-pass scan produced.
- [ ] 1.4 Verify: `mise run build` and `mise run test` pass.

## 2. Extend the lint engine

- [ ] 2.1 Add `workflows_full: &'ctx [Parsed]` field to `Context`. Existing rules continue to use `workflows`.
- [ ] 2.2 Add six variants to `RuleName` enum: `MissingPermissions`, `ExcessivePermissions`, `DangerousTrigger`, `PrHeadCheckout`, `MissingConcurrency`, `UnprotectedSecrets`. Update `FromStr`, `Display`, and the kebab-case serde rename.
- [ ] 2.3 Update `command.rs` lint-command runner to instantiate the six new rules and pass `workflows_full` in the context.
- [ ] 2.4 Update `Diagnostic` to optionally carry job/step location (some rules emit step-scoped diagnostics).
- [ ] 2.5 Verify: `mise run build` passes.

## 3. Implement the six rules

- [ ] 3.1 `src/lint/missing_permissions.rs` — error if `Parsed.permissions` is `None`. Unit tests: clean workflow, missing block, top-level present.
- [ ] 3.2 `src/lint/excessive_permissions.rs` — warn if top-level `permissions:` declares anything other than `contents: read`. Unit tests: `contents: read` only (clean), `contents: write` at top level (warns), `permissions: write-all` (warns), `permissions: read-all` (warns — broader than `contents: read`).
- [ ] 3.3 `src/lint/dangerous_trigger.rs` — error if `Parsed.on` contains `pull_request_target` OR `workflow_run`. Diagnostic message names the specific trigger so the user can act on the right line. Unit tests: pull_request only (clean), pull_request_target (errors), workflow_run (errors), both pull_request_target + workflow_run in same file (two diagnostics), workflow_run with ignore opt-out (clean).
- [ ] 3.4 `src/lint/pr_head_checkout.rs` — error if (any job has a write permission OR any step references `secrets.*`) AND (any step has `with.ref` matching `github.event.pull_request.head.sha` / `github.head_ref` / `github.event.pull_request.head.ref`). Unit tests: privileged + PR-head ref (errors), non-privileged + PR-head ref (clean), privileged without ref (clean).
- [ ] 3.5 `src/lint/missing_concurrency.rs` — warn if `Parsed.on` contains `push` or `schedule` and `Parsed.concurrency` is `None`. Unit tests: push without concurrency (warns), push with concurrency (clean), pull_request without concurrency (clean).
- [ ] 3.6 `src/lint/unprotected_secrets.rs` — error per the design.md algorithm. `secrets.GITHUB_TOKEN` is excluded from the matched set (auto-scoped down on fork PRs by GitHub). Unit tests: PR workflow + user secret + no gate (errors), PR workflow + user secret + correct gate (clean), PR workflow + `secrets.GITHUB_TOKEN` + no gate (clean — excluded), pull_request_target + secret (clean — `dangerous-trigger` covers this), workflow_run + secret (clean — `dangerous-trigger` covers this), non-PR workflow + secret (clean), PR workflow + user secret + custom non-canonical gate (errors — opt out via `ignore`).
- [ ] 3.7 Verify: `mise run test` passes; each rule has the four standard test shapes (clean, fails, ignore-scopes, disabled).

## 4. Configuration integration

- [ ] 4.1 Confirm the existing `[lint.rules]` TOML parser accepts the six new rule names (it parses `RuleName` via serde — adding enum variants is enough).
- [ ] 4.2 Confirm the existing `ignore = [{ action = ..., workflow = ..., job = ... }]` mechanism works for workflow-scoped rules. The `action` key is meaningless for `missing-permissions` and similar — document that omitting it is fine.
- [ ] 4.3 Add integration tests that exercise `level = "off"` on each new rule.

## 5. Output and CLI

- [ ] 5.1 Confirm `gx lint --help` lists the new rules (or links to the docs if `--help` is short).
- [ ] 5.2 Update the summary line to count all rule categories uniformly.
- [ ] 5.3 Verify diagnostic ordering is stable: sorted by `(workflow_path, job_id, step_index, rule_name)`.

## 6. Documentation

- [ ] 6.1 Update README's "Commands" section to mention security rules in the `gx lint` row.
- [ ] 6.2 Update `docs/demo.tape` if the demo flow showcases lint (otherwise skip — `demo.tape` mostly demos `tidy`).
- [ ] 6.3 Add a `docs/lint-rules.md` (or extend the existing lint doc) with one paragraph per rule: what it catches, the canonical fix, how to opt out via ignore.

## 7. Release

- [ ] 7.1 Update CHANGELOG with a `### Added` section enumerating the six rules and a `### Breaking` note that any of them can be set to `level = "off"` if the user wants to opt out.
- [ ] 7.2 Bump the version in `Cargo.toml`. Per gx's convention, this is a minor bump (new functionality, technically breaking by virtue of new error-level diagnostics, but the `level = "off"` escape hatch keeps it minor by SemVer convention for "tool" packages).
- [ ] 7.3 Verify: `mise run lint` and `mise run test` pass; `cargo build --release` succeeds.

## 8. Downstream coordination

- [ ] 8.1 After release, open a PR on `gmeligio/flutter-docker-image` that bumps the pinned gx version and configures the six rules at their default levels in `.github/gx.toml`. The PR's CI run validates that the rules catch the real workflow corpus.
