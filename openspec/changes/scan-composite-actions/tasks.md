## 1. Failing regression tests (must fail on current main)

- [ ] 1.1 Add `write_composite_action(root, name, content)` to `tests/common/setup.rs` beside `write_workflow` (`setup.rs:29`); it writes `.github/actions/<name>/action.yml`.
- [ ] 1.2 In `tests/integ_tidy.rs`, add a test: an action referenced only from `.github/actions/setup/action.yml` is present in `gx.toml` and `gx.lock` after `gx tidy`, and the `action.yml` is rewritten with the resolved SHA + `# v…` comment. Confirm it FAILS on current code (the action is pruned and the file untouched).
- [ ] 1.3 In `tests/integ_upgrade.rs`, add a test: a pin inside `.github/actions/setup/action.yml` is advanced by `gx upgrade` and the file rewritten. Confirm it FAILS (guards design D4 — `update_all_with_pins` globs its own workflow-only list at `src/infra/workflow_update.rs:85`).
- [ ] 1.4 In `tests/integ_lint.rs`, add tests: `unpinned` fires on a composite step and the diagnostic carries `.github/actions/setup/action.yml` with a line; `unsynced-manifest` does NOT report an action referenced only from a composite; `missing-permissions` does NOT fire on any composite file; an `ignore = [{ workflow = ".github/actions/setup/action.yml" }]` entry suppresses the diagnostic. Confirm each FAILS or is vacuous on current code.
- [ ] 1.5 In `src/infra/workflow_scan/tests.rs`, add discovery tests: composite found at `.github/actions/foo/action.yml`; found nested at `.github/actions/a/b/action.yml`; `action.yaml` extension found; sibling `config.yml` NOT read; `runs.using: node20` yields zero actions and no error; malformed composite YAML yields `Err` without aborting the scan (mirror `tests.rs:448`).
- [ ] 1.6 Add extraction tests: a composite step yields `Location { job: None, step: Some(i), line: Some(n) }`; local `./…` and `docker://` refs inside a composite are skipped (extend `scan_skips_local`, `tests.rs:103`); per-step version comments stay distinct.
- [ ] 1.7 Run `mise run test` and record which tests fail and why, confirming each failure is the absence of the feature rather than a broken fixture.

## 2. Parse the composite schema

- [ ] 2.1 In `src/domain/workflow_parsed/mod.rs`, add a `Runs` wire shape capturing `using: Option<String>` and `steps: Vec<Step>`; reuse the existing `Step` struct unchanged (it already carries `uses`, `run`, `shell`, `with`, `env`).
- [ ] 2.2 Add a file-kind discriminant to `Parsed` (design D1): `Workflow` carrying the existing `jobs`, vs `CompositeAction` carrying `runs.steps`. `runs.using` values other than `composite` yield a composite kind with an empty step list — not an error (spec: "Only composite actions contribute managed references").
- [ ] 2.3 Decide the kind from the file path at parse entry (`Parsed::from_yaml` takes the path already), not from YAML shape sniffing — a workflow and an action definition are distinguished by location, and a malformed file must still be attributed to the right schema for its error message.
- [ ] 2.4 Add unit tests in `src/domain/workflow_parsed/tests.rs`: a `runs.using: composite` document parses to the composite kind with its steps; a workflow document still parses to the workflow kind with its jobs; a `node20` action parses with zero steps.

## 3. Discover and extract composite files

- [ ] 3.1 In `src/infra/workflow_scan/scanner.rs`, replace the single `workflows_dir` field (`scanner.rs:113`) with the two discovery roots from design D3: `.github/workflows/*.{yml,yaml}` (non-recursive) and `.github/actions/**/action.{yml,yaml}` (recursive). Keep order deterministic: workflows first, then composites, each sorted.
- [ ] 3.2 Extend `find_workflow_files` (`scanner.rs:80`) to glob both roots, preserving the existing behavior that a bad glob is a `ScanFailed` and per-file read/parse problems are per-file errors.
- [ ] 3.3 In `extract_workflow` (`scanner.rs:146`), select the step list by kind: `jobs[].steps[]` with `job: Some(id)` for workflows, `runs.steps[]` with `job: None` for composites. Leave the `uses:` handling below the step lookup untouched — same `USES_RE`, same `.`/`docker://` skip (`scanner.rs:176`), same `UsesRef::new`.
- [ ] 3.4 Update the `Scanner` trait docs in `src/domain/workflow.rs:27-76` — "workflow files" becomes "managed files (workflows and composite actions)". Method signatures and object-safety are unchanged; `scan_all_with_parsed` still returns one `Parsed` per file, now kind-tagged.
- [ ] 3.5 Confirm tasks 1.5 and 1.6 now pass.

## 4. Write pins to composite files

- [ ] 4.1 In `src/infra/workflow_update.rs`, make `update_all_with_pins` (`:85`) take its file list from the same discovery source as the scanner rather than its own hardcoded `.github/workflows` glob (`:31`, `:40-58`) — design D4, one discovery source. `apply_patches` (`:65`) already takes explicit paths; leave it alone.
- [ ] 4.2 Verify the regex rewrite path (`:128`) needs no change for composite files (it is text-based, indentation-agnostic) — add an assertion in the upgrade test that indentation and surrounding YAML are preserved.
- [ ] 4.3 In `src/tidy/patches.rs:34`, confirm `find_workflow_paths()` now yields composite paths so tidy patches reach them.
- [ ] 4.4 Confirm tasks 1.2 and 1.3 now pass.

## 5. Scope the lint rules

- [ ] 5.1 In `src/lint/command.rs:65`, filter `scan_all_with_parsed`'s parses so `Context.workflows_full` (`src/lint/rule.rs:153`) carries workflow-kind files only (design D2 — boundary filter, not per-rule guards). Update the field doc to state the invariant.
- [ ] 5.2 Verify `Context.workflows` and `Context.action_set` DO include composite-derived actions, so `unpinned`, `sha-mismatch`, `stale-comment`, and `unsynced-manifest` cover them with no rule-code change.
- [ ] 5.3 Check the diagnostic sort key in `src/lint/command.rs:163-170` — a `None` job sorts first; confirm composite diagnostics interleave sensibly with workflow ones and that output ordering is stable across runs.
- [ ] 5.4 Confirm `matches_ignore_workflow`'s suffix match (`src/lint/rule.rs:207,217`) accepts a composite path, and that an ignore entry naming a `job` never matches a composite diagnostic (no job to match).
- [ ] 5.5 Confirm task 1.4 now passes.

## 6. Manifest override vocabulary

- [ ] 6.1 In `src/infra/manifest/convert.rs:115-120`, relax the "`step` requires `job`" validation so a composite-file override `{ workflow = "…/action.yml", step = N }` is accepted (design D5). Keep the duplicate-scope check (`:123-130`) intact.
- [ ] 6.2 Verify three-tier override resolution (`src/domain/manifest/overrides.rs:31-67`) and `prune_stale` (`:135`) handle a `job: None` located action — a composite override must survive pruning while its file still references the action.
- [ ] 6.3 Add unit tests for a composite step override applying, and for a stale composite override being pruned when the file no longer references the action.

## 7. Output and reporting

- [ ] 7.1 Confirm composite files rewritten are counted in `workflows_updated` (`src/tidy/report.rs:8`) so `gx tidy`/`gx upgrade` summaries reflect the writes (design Observability). Keep the `--json` field name to avoid a breaking output change (`docs/renovate.md:120`).
- [ ] 7.2 Change the human-facing summary noun from "workflows" to "files" where it now covers both kinds; leave the JSON key alone.
- [ ] 7.3 Verify `src/output/lines.rs:111-118` renders `.github/actions/foo/action.yml:12` correctly with no change.

## 8. Re-scope the conflicting existing test

- [ ] 8.1 Rewrite `tests/integ_tidy.rs::gx_tidy_skips_local_actions` (`:345`). Its current assertion that the manifest omits `.github/actions` (`:372`) conflates "local reference" with "composite file". It must assert only that the local `./.github/actions/foo` *reference* is absent from the manifest, while an action referenced by SHA/tag inside that same composite file IS present.

## 9. Docs and verification

- [ ] 9.1 Update `README.md` to state which files gx manages — `.github/workflows/*.yml` and `.github/actions/**/action.yml` — since discovery was previously undocumented and is now a spec contract.
- [ ] 9.2 Update `docs/lint-rules.md`: describe the `workflow` ignore key as a file path matching either kind (`lint-rules.md:16,118,127`), and note that workflow-schema rules do not fire on composite action files.
- [ ] 9.3 Update `docs/demo.tape` only if the demo should show a composite action being pinned (per `AGENTS.md`); record the decision either way.
- [ ] 9.4 Note the behavioral break in the changelog entry: `gx tidy` now adds and rewrites composite-action references it previously pruned, and `gx lint` may report new `unpinned` diagnostics.
- [ ] 9.5 Run `mise run test`, `mise run clippy:check`, and the code-health gates (`tests/code_health.rs`) — clean.
- [ ] 9.6 Manually reproduce issue #150's scenario end to end: create a repo with a composite action, run `gx tidy`, confirm the actions are added rather than removed, and record the actual output here.
