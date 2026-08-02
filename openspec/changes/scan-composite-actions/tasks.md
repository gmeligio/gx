## 1. Failing regression tests (must fail on current main)

- [x] 1.1 Add `write_composite_action(root, name, content)` to `tests/common/setup.rs` beside `write_workflow` (`setup.rs:29`); it writes `.github/actions/<name>/action.yml`.
- [x] 1.2 In `tests/integ_tidy.rs`, add a test: an action referenced only from `.github/actions/setup/action.yml` is present in `gx.toml` and `gx.lock` after `gx tidy`, and the `action.yml` is rewritten with the resolved SHA + `# v…` comment. Confirm it FAILS on current code (the action is pruned and the file untouched).
- [x] 1.3 In `tests/integ_upgrade.rs`, add a test: a pin inside `.github/actions/setup/action.yml` is advanced by `gx upgrade` and the file rewritten. Confirm it FAILS (guards design D4 — `update_all_with_pins` globs its own workflow-only list at `src/infra/workflow_update.rs:85`).
- [x] 1.4 In `tests/integ_lint.rs`, add tests: `unpinned` fires on a composite step and the diagnostic carries `.github/actions/setup/action.yml` with a line; `unsynced-manifest` does NOT report an action referenced only from a composite; `missing-permissions` does NOT fire on any composite file; an `ignore = [{ workflow = ".github/actions/setup/action.yml" }]` entry suppresses the diagnostic. Confirm each FAILS or is vacuous on current code.
- [x] 1.5 In `src/infra/workflow_scan/tests.rs`, add discovery tests: composite found at `.github/actions/foo/action.yml`; found nested at `.github/actions/a/b/action.yml`; `action.yaml` extension found; sibling `config.yml` NOT read; `runs.using: node20` yields zero actions and no error; `runs.steps` present but `runs.using` absent yields zero actions and no error; malformed composite YAML yields `Err` without aborting the scan (mirror `tests.rs:448`).
- [x] 1.6 Add extraction tests: a composite step yields `Location { job: None, step: Some(i), line: Some(n) }`; local `./…` and `docker://` refs inside a composite are skipped (extend `scan_skips_local`, `tests.rs:103`); per-step version comments stay distinct.
- [x] 1.7 Run `mise run test` and record which tests fail and why, confirming each failure is the absence of the feature rather than a broken fixture.

## 2. Parse the composite schema

- [x] 2.1 In `src/domain/workflow_parsed/mod.rs`, add a `Runs` wire shape capturing `using: Option<String>` and `steps: Vec<Step>`; reuse the existing `Step` struct unchanged (it already carries `uses`, `run`, `shell`, `with`, `env`).
- [x] 2.2 Add a file-kind discriminant to `Parsed` (design D1): `Workflow` carrying the existing `jobs`, vs `CompositeAction` carrying `runs.steps`. `runs.using` values other than `composite` yield a composite kind with an empty step list — not an error (spec: "Only composite actions contribute managed references").
- [x] 2.3 Decide the kind from the file path at parse entry (`Parsed::from_yaml` takes the path already), not from YAML shape sniffing — a workflow and an action definition are distinguished by location, and a malformed file must still be attributed to the right schema for its error message.
- [x] 2.4 Add unit tests in `src/domain/workflow_parsed/tests.rs`: a `runs.using: composite` document parses to the composite kind with its steps; a workflow document still parses to the workflow kind with its jobs; a `node20` action parses with zero steps.

## 3. Discover and extract composite files

- [ ] 3.1 Extract discovery into a free function over `repo_root` (design D4 mechanism) that returns the ordered file list with each path's kind, and have `FileScanner` (`scanner.rs:110-115`) call it instead of holding a `workflows_dir` field.
- [ ] 3.2 Add the composite root to that function: `.github/actions/**/action.{yml,yaml}`, recursive, alongside the existing non-recursive `.github/workflows/*.{yml,yaml}`. Preserve the existing behavior that a bad glob is a `ScanFailed` and per-file read/parse problems are per-file errors.
- [ ] 3.3 Sort within each group and emit workflows before composites, replacing today's all-`*.yml`-then-all-`*.yaml` filesystem order (`scanner.rs:83-95`). Add a test asserting two runs over the same tree yield identical order, and that a workflow sorts before a composite.
- [ ] 3.4 In `extract_workflow` (`scanner.rs:146`), select the step list by kind: `jobs[].steps[]` with `job: Some(id)` for workflows, `runs.steps[]` with `job: None` for composites. Leave the `uses:` handling below the step lookup untouched — same `USES_RE`, same `.`/`docker://` skip (`scanner.rs:176`), same `UsesRef::new`.
- [ ] 3.5 Update the `Scanner` trait docs in `src/domain/workflow.rs:27-76` — "workflow files" becomes "managed files (workflows and composite actions)". Method signatures and object-safety are unchanged; `scan_all_with_parsed` still returns one `Parsed` per file, now kind-tagged.
- [ ] 3.6 Confirm tasks 1.5 and 1.6 now pass.

## 4. Write pins to composite files

- [ ] 4.1 In `src/infra/workflow_update.rs`, make `update_all_with_pins` (`:85`) call the shared discovery function from 3.1 instead of its own hardcoded `.github/workflows` glob (`:31`, `:40-58`) — design D4, one discovery source. `apply_patches` (`:65`) already takes explicit paths; leave it alone.
- [ ] 4.2 Add an assertion to the 1.3 upgrade test that the rewritten `action.yml` preserves indentation and all surrounding YAML, confirming the regex path (`:128`) needs no schema awareness.
- [ ] 4.3 Add a test that `find_workflow_paths()` yields the composite file path, so `src/tidy/patches.rs:34` reaches it.
- [ ] 4.4 Confirm tasks 1.2 and 1.3 now pass.

## 5. Scope the lint rules

- [ ] 5.1 In `src/lint/command.rs:65`, filter `scan_all_with_parsed`'s parses so `Context.workflows_full` (`src/lint/rule.rs:153`) carries workflow-kind files only (design D2 — boundary filter, not per-rule guards). Update the field doc to state the invariant.
- [ ] 5.2 Add a test asserting `Context.workflows` and `Context.action_set` DO include composite-derived actions, so `unpinned`, `sha-mismatch`, `stale-comment`, and `unsynced-manifest` cover them with no rule-code change. Also assert `run-shellcheck` sees no composite file (the deferred opt-in stated in the lint delta).
- [ ] 5.3 Add a test that a repository mixing workflow and composite diagnostics emits them in the same order on repeated runs, exercising the `None`-job branch of `diagnostic_sort_key` (`src/lint/command.rs:163-170`).
- [ ] 5.4 Add tests that `matches_ignore_workflow`'s suffix match (`src/lint/rule.rs:207,217`) accepts a composite path, and that an ignore entry naming a `job` never suppresses a composite diagnostic (no job to match).
- [ ] 5.5 Confirm task 1.4 now passes.

## 6. Manifest override vocabulary

- [ ] 6.1 Add a failing unit test first: an override `{ workflow = ".github/actions/setup/action.yml", step = 0 }` applies to the located action at step 0 of that file. It fails today because `resolve_version`'s workflow-level tier requires `exc.step.is_none()` (`src/domain/manifest/overrides.rs:61`), so the override matches no tier.
- [ ] 6.2 In `src/infra/manifest/convert.rs:115-120`, relax the "`step` requires `job`" validation so a composite-file override `{ workflow = "…/action.yml", step = N }` is accepted (design D5). Keep the duplicate-scope check (`:123-130`) intact.
- [ ] 6.3 In `resolve_version` (`overrides.rs:31-67`), add the file+step tier: workflow matches, `exc.job.is_none()`, `exc.step == location.step`. Order it after the job+step tier and before the workflow-level tier. Update the resolution-order doc comment (`:23-29`) from three tiers to four.
- [ ] 6.4 In `prune_stale` (`overrides.rs:162-171`), add the `(None, Some(step))` case: a composite override survives only while a located action exists at that file and step index with `job: None`. Today it survives on file-path match alone, outliving the step it names.
- [ ] 6.5 Add unit tests: a composite step override applies (6.1 now passes); a file-level override still wins where no step override matches; a composite override is pruned when the file no longer references that step; a job-bearing override is unaffected by the new tier.
- [ ] 6.6 Add a round-trip test for `sync`-generated composite overrides: `sync` (`overrides.rs:112-128`) copies `location.job`/`location.step` verbatim, so with composite locations it emits `{ workflow, step, job: None }` itself. Assert such an entry is written by `patch.rs` and read back by `convert.rs` without error — without 6.2, gx would write a manifest it then refuses to parse.

## 7. Output and reporting

- [ ] 7.1 Add a test asserting a mixed run (two workflows + one composite rewritten) reports 3 in `workflows_updated` (`src/tidy/report.rs:16`), so summaries reflect the writes. Keep the `--json` field name to avoid a breaking output change (`docs/renovate.md:120`).
- [ ] 7.2 Change the human-facing summary noun from "workflows" to "files" in `src/tidy/report.rs:66` and `src/upgrade/report.rs:118`, and update the two unit-test expectations that pin the old wording (`tidy/report.rs:117` `"… · 2 workflows"`, `upgrade/report.rs:184` `"2 upgraded · 1 workflow"`). Leave the JSON key alone. Backed by the `file-discovery` requirement "The summary counts files, not workflows".
- [ ] 7.3 Add an assertion to the 1.4 lint test that the rendered line reads `.github/actions/setup/action.yml:<n>:`, confirming `src/output/lines.rs:111-118` needs no change.

## 8. Re-scope the conflicting existing test

- [ ] 8.1 Rewrite `tests/integ_tidy.rs::gx_tidy_skips_local_actions` (`:345`). Its current assertion that the manifest omits `.github/actions` (`:372`) conflates "local reference" with "composite file". It must assert only that the local `./.github/actions/foo` *reference* is absent from the manifest, while an action referenced by SHA/tag inside that same composite file IS present.

## 9. Docs and verification

- [ ] 9.1 Update `README.md` to state which files gx manages — `.github/workflows/*.yml` and `.github/actions/**/action.yml` — since discovery was previously undocumented and is now a spec contract.
- [ ] 9.2 Update `docs/lint-rules.md`: describe the `workflow` ignore key as a file path matching either kind (`lint-rules.md:16,118,127`), and note that workflow-schema rules do not fire on composite action files.
- [ ] 9.3 Update `docs/demo.tape` if the demo repository gains a composite action (its setup block at `demo.tape:11-26` currently creates only `.github/workflows/ci.yml`); if it does not, record that no change was needed and why.
- [ ] 9.4 Note the behavioral break in the changelog entry: `gx tidy` now adds and rewrites composite-action references it previously pruned, and `gx lint` may report new `unpinned` diagnostics.
- [ ] 9.5 Run `mise run test`, `mise run clippy:check`, and the code-health gates (`tests/code_health.rs`) — clean.
- [ ] 9.6 Manually reproduce issue #150's scenario end to end: create a repo with a composite action, run `gx tidy`, confirm the actions are added rather than removed, and record the actual output here.
