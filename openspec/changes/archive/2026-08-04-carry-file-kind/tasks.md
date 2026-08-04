## 1. Pin current behavior before changing it

- [x] 1.1 Add an integration assertion that `gx lint` output is unchanged for a fixture repo containing both workflows and `.github/actions` composites — the no-op guarantee for every repo today. This is the safety net for groups 2–5.
- [x] 1.2 Run `mise run test:all` and confirm 1.1 passes against unmodified code, so a later failure means the change broke something rather than the test being wrong.

## 2. Split `Parsed` into a real sum type

- [x] 2.1 Create the new module (D6) and move `FileKind` into it, leaving `parsed/mod.rs` re-exporting so existing imports keep working. Confirm `mise run lint:size` still passes before adding anything — `parsed/mod.rs` is at 431 lines against a 440 logic-line budget.
- [x] 2.2 In the new module, define `ParsedWorkflow { path, on, permissions, concurrency, defaults, jobs }` and `ParsedAction { path, steps }` as structs, and `Parsed` as the enum over them with `path()` reachable from either variant. Every field needs a `///` — `missing_docs_in_private_items` is on.
- [x] 2.3 Rewrite `Parsed::parse` to build the variant from the caller-supplied `kind`, absorbing the existing `match kind { Workflow => …jobs, ActionDefinition => …steps }` at `scanner.rs:185-208`. Keep the non-composite `runs.using` case yielding an action with no steps — not an error.
- [x] 2.4 Keep `Parsed::from_yaml` (`parsed/mod.rs:378-380`) as a workflow-only constructor so the parse test surface does not churn.
- [x] 2.5 Update `src/domain/file/parsed/tests.rs` for the new shape, including `parsed/tests.rs:458-461` which asserts `of_path` behavior that is about to be deleted.

## 3. Delete the second derivation

- [x] 3.1 Delete `FileKind::of_path` (`parsed/mod.rs:290-309`) and fix the two production call sites the compiler reports: `src/infra/workflow_scan/scanner.rs:224` and `src/infra/manifest/convert.rs:148`.
- [x] 3.2 In `src/infra/workflow_scan/scanner.rs`, thread `ManagedFile.kind` from discovery through to `Parsed` construction so the kind assigned at `discovery.rs:82-85` is the kind parsed with. Do not add a kind field to `site::Id` (D2). — `scan` and `scan_all_with_parsed` already passed `file.kind`; the only path deriving kind from a path was `scan_file`, which now takes it as a parameter.
- [x] 3.3 Delete the drift-guard test `discovery_kind_agrees_with_of_path` (`src/infra/workflow_scan/composite_tests.rs:299-323`) — with one derivation there is nothing to agree with. Replace it with a test asserting kind is what discovery said, read back off the parsed file.
- [x] 3.4 Update the `discovery.rs:22-23` doc comment, which currently explains the drift risk between the two derivations, and `discovery.rs:1`'s "single source of truth" claim — now literally true.

## 4. Make bare-step override validation shape-only

- [x] 4.1 Change `src/infra/manifest/convert.rs:147-158` to map a `step`-without-`job` override to `Scope::CompositeStep` on any path, dropping the `of_path` question. Do not substitute an inline path-shape heuristic — that is `of_path` under another name (design D4).
- [x] 4.2 Invert `step_without_job_on_a_workflow_is_rejected` (`src/infra/manifest/override_scope_tests.rs:16`): it must now assert the override parses and then selects no site on a workflow, rather than asserting a parse error. Update its doc comment, which states the old rationale.
- [x] 4.3 Confirm `step_without_job_on_a_composite_action_is_accepted` (`override_scope_tests.rs:39`) still passes unchanged, and that the write/read round-trip test at `override_scope_tests.rs:95-124` still holds.
- [x] 4.4 Do not touch lint `ignore` targets (#162) or `prune_stale` reporting (#163) — both are out of scope; see design D4's rejected alternatives.

## 5. Make the `workflows_full` invariant structural

- [x] 5.1 In `src/lint/rule.rs`, retype `Context.workflows_full` to the real `ParsedWorkflow` struct, removing the `Parsed` type alias at `rule.rs:8`. Replace the prose invariant comment at `rule.rs:152` with a note that it is now a compile error to violate.
- [x] 5.2 In `src/lint/command.rs:69-72`, replace the `.filter(|p| p.kind == FileKind::Workflow)` with a partition producing `Vec<ParsedWorkflow>`, so deleting it stops compiling.
- [x] 5.3 Update the eight workflow-schema rules the compiler now flags — `dangerous_trigger`, `excessive_permissions`, `missing_concurrency`, `missing_permissions`, `pr_head_checkout`, `unprotected_secrets`, `dangling_reference`, `invalid_expression` — signature only, no body changes.
- [x] 5.4 Update `src/lint/run_shellcheck/mod.rs:51`. Do not extend it to composite `run:` bodies — that stays deferred to #160; the narrower type makes the gap explicit.
- [x] 5.5 Confirm the three `workflows_full: &[]` sites in `src/lint/stale_comment.rs` (149, 178, 210) and `src/lint/run_shellcheck/tests.rs:146` still compile.

## 6. Cover the #124 precondition

- [x] 6.1 Add a test that a file classified `ActionDefinition` whose path is outside `.github/actions` parses under the action schema and yields its `runs.steps` references. Drive it by constructing the kind directly — kind comes from the caller, not the path, which is the point.
- [x] 6.2 Add a test that such a file is absent from `workflows_full` and produces no workflow-schema diagnostics, matching the new `lint-command` scenario.
- [x] 6.3 Add a test that `.github/workflows/action.yml` is still read as a workflow (kind follows discovery, not file name).

## 7. Gate and document

- [x] 7.1 Run `mise run format`, then `mise run test` — budget for the strict wall: `missing_docs_in_private_items` on every new private struct and its fields, `too_many_lines`, and fulfilled `#[expect(...)]`. Keep any `#[cfg(test)] mod tests` at the very bottom of its file.
- [x] 7.2 Run `mise run test:all` and confirm 1.1's no-op assertion still holds — `gx lint`, `gx tidy`, `gx upgrade` output unchanged on a conventional repo.
- [x] 7.3 Add a CHANGELOG entry under Changed for the D4 validation change, so the vanished parse-time message is not read as a regression. Note that the case it covered is picked up by #163.
- [x] 7.4 Comment on #154 that this landed, and on #124 that its blocker is clear.
