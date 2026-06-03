## 1. Extend the workflow parse model

- [ ] 1.1 In `src/domain/workflow_parsed/mod.rs`, add `needs: Vec<String>` to `Job` (and its wire struct). Deserialize the scalar-or-sequence union (`needs: build` and `needs: [build, test]`) into `Vec<String>`; absent → empty vec. Mirror the custom-deserialize pattern used by `JobSecrets`.
- [ ] 1.2 Add `outputs: BTreeMap<String, String>` to `Job` (`#[serde(default)]`). The `invalid-expression` rule uses the key set; values are retained for a future key-level check.
- [ ] 1.3 Add `id: Option<String>` to `Step` (`#[serde(default)]`).
- [ ] 1.4 Add deserialization unit tests in `src/domain/workflow_parsed/tests.rs`: `needs` scalar form, `needs` sequence form, absent `needs`, step with/without `id`, job with/without `outputs`.

## 2. Add the `dangling-reference` rule

- [ ] 2.1 Add `DanglingReference` to `RuleName` (`src/lint/rule.rs`): enum variant, `Display` → `dangling-reference`, `FromStr`, and the `rule_name_*` roundtrip tests.
- [ ] 2.2 Create `src/lint/dangling_reference.rs`: a `Rule` impl, `default_level()` = `Error`. In `check`, for each workflow build the set of job ids; for each job, flag any `needs:` entry not in that set. Emit a `Diagnostic` scoped with `.with_workflow(...).with_job(...)` naming the missing id.
- [ ] 2.3 Register the rule in `src/lint/command.rs` alongside the workflow-security family (same `run_workflow_rule` path).
- [ ] 2.4 Unit tests: missing job flagged; scalar `needs` to a real job not flagged; sequence with one bad entry flags only the bad one; `off` level suppresses.

## 3. Add the `invalid-expression` rule

- [ ] 3.1 Add `InvalidExpression` to `RuleName` with the same wiring + roundtrip tests as 2.1.
- [ ] 3.2 Create `src/lint/invalid_expression.rs`: a `Rule` impl, `default_level()` = `Error`. For each job, build the declared-step-id set incrementally as steps are walked in order. Scan each `${{ ... }}` span in `if_cond`/`with`/`env`/`run` (reuse `Step::scalar_text` where it fits, but note it concatenates — for accurate per-reference reporting, scan the individual fields) for anchored `needs.<id>.` and `steps.<id>.` bare-identifier patterns.
- [ ] 3.3 Resolution logic: `needs.<id>` is invalid if `<id>` is not in the enclosing job's `needs:` set (including the case where the job declares no `needs:` at all). `steps.<id>` is invalid if `<id>` is not in the set of ids declared by *earlier* steps in the same job.
- [ ] 3.4 Conservative matching: a `${{ }}` span whose `needs`/`steps` access is not a bare-identifier dotted form (e.g. `needs[matrix.x]`, `steps[format(...)]`) is skipped. Contexts other than `needs.`/`steps.` are skipped.
- [ ] 3.5 Register in `src/lint/command.rs`.
- [ ] 3.6 Unit tests covering each spec scenario: undeclared-needs job, nonexistent step id, valid step id (no flag), later-step id (flagged), dynamic reference (no flag), out-of-scope context (no flag), `off` suppresses.

## 4. Docs and changelog

- [ ] 4.1 Add a "Workflow validity" subsection to `docs/lint-rules.md` documenting both rules, their defaults, and `ignore` scoping (workflow/job; `action` key meaningless).
- [ ] 4.2 Note both new default-error rules in the changelog / release notes path (`release-plz` picks up the conventional-commit body — call out the breaking-for-CI nature).
- [ ] 4.3 Update README's lint-rule listing if it enumerates rules.

## 5. Validate

- [ ] 5.1 `openspec validate add-workflow-validity-rules --strict`.
- [ ] 5.2 `mise run test` (per AGENTS.md — never invoke cargo directly) green, including the new unit tests.
- [ ] 5.3 `mise run lint` (gx's own clippy/fmt task) clean.
- [ ] 5.4 Dogfood: run the built `gx lint` against a fixture workflow with a deliberately dangling `needs:` and a typo'd `steps.<id>.outputs` reference; confirm both rules fire and that flutter-docker-image's real `update-version.yml` (post-p12) lints clean.
