## 1. Extend the parse model for shell detection

- [x] 1.1 In `src/domain/workflow_parsed/mod.rs`, add `shell: Option<String>` to `Step` (and its wire struct) with `#[serde(default)]`.
- [x] 1.2 Add `defaults.run.shell` at workflow and job level: a small `Defaults { run: Option<RunDefaults> }` with `RunDefaults { shell: Option<String> }`, `#[serde(default)]` on both, on `Parsed` and `Job` (and their wire structs).
- [x] 1.3 Add an effective-shell resolver: `step.shell → job defaults.run.shell → workflow defaults.run.shell → "bash"`. Normalize prefix forms (`bash -e {0}` → `bash`, `sh ...` → `sh`).
- [x] 1.4 Deserialization + resolver tests: step `shell: bash`/`pwsh`/absent; `defaults.run.shell` at job and workflow level; precedence (step overrides job overrides workflow); absent-everywhere → bash.

## 2. shellcheck invocation behind a `ShellChecker` seam

- [x] 2.1 Define `trait ShellChecker { fn check(&self, script: &str, shell: Sh) -> Vec<Finding> }` and a typed `Finding { code: u16, level, line, column, message }` under `src/infra/`. Newtype the shellcheck JSON at the edge (serde), not in rule logic.
- [x] 2.2 Implement `ShellcheckCli`: invoke `shellcheck --norc -f json -x --shell <sh> -e SC1091,SC2194,SC2050,SC2153,SC2154,SC2157,SC2043 -` with the script on **stdin**. Prepend the runtime setup line (`set -eo pipefail` for bash, `set -e` for sh) and offset reported lines by `-1`.
- [x] 2.3 Model binary availability as `enum Availability { Present(ShellcheckCli), Absent }`, probed once per lint run (not per step) — distinct from "ran, here are findings" so the rule degrades gracefully.
- [x] 2.4 Add `sanitize_expressions(&str) -> String`: replace each `${{ ... }}` span with an equal-length run of underscores (preserve columns). Port from actionlint `rule_shellcheck.go`.
- [x] 2.5 Add a `FakeChecker` test double returning canned `Finding`s, and unit-test the JSON parser against a captured shellcheck `-f json` sample (no live binary in unit tests).

## 3. Add the `run-shellcheck` rule

- [ ] 3.1 Add `RunShellcheck` to `RuleName` (`src/lint/rule.rs`): variant, `Display` → `run-shellcheck`, `FromStr`, roundtrip tests.
- [ ] 3.2 Create `src/lint/run_shellcheck.rs`: `Rule` impl, `default_level()` = `Warn`. The rule holds a `&dyn ShellChecker` (or `Availability`). In `check`: if `Absent`, emit one informational skip diagnostic and return. Otherwise, for each workflow/job/step where `run:` is present and the effective shell (task 1.3) is bash/sh: `sanitize_expressions` the body, run it through the checker, and map each `Finding` to a `Diagnostic` scoped `.with_workflow().with_job().with_step()`, message including the `SCxxxx` code and the in-script line.
- [ ] 3.3 Skip steps whose effective shell is a non-shell (`pwsh`, `python`, `cmd`, ...).
- [ ] 3.4 Register the rule in `src/lint/command.rs` (same `run_workflow_rule` path as the workflow-validity family).
- [ ] 3.5 Unit tests for each spec scenario, driving the rule with `FakeChecker` (no live binary): finding reported at warn (exit 0); escalated to error (exit 1); clean body (no diagnostic); `${{ }}` expression does not produce a false positive; binary-missing (`Availability::Absent`) skip diagnostic + no failure; non-shell step skipped; `defaults.run.shell: pwsh` skips an absent-`shell:` step; `ignore` by workflow/job suppresses.

## 4. Toolchain, docs, changelog

- [ ] 4.1 Add `shellcheck` to gx's own toolchain (`mise.toml` / `.config`) so gx's CI runs the rule against gx's workflows.
- [ ] 4.2 Document `run-shellcheck` in `docs/lint-rules.md`: default warn, the `shellcheck` binary requirement, the graceful-skip behavior, and `ignore` scoping (workflow/job/step).
- [ ] 4.3 Note the new rule + optional `shellcheck` dependency in the changelog / release-notes path.
- [ ] 4.4 Update README's lint-rule listing if it enumerates rules.

## 5. Validate

- [ ] 5.1 `openspec validate add-run-shellcheck-rule --strict`.
- [ ] 5.2 `mise run test` (per AGENTS.md) green, including new unit tests.
- [ ] 5.3 `mise run lint` clean.
- [ ] 5.4 Dogfood: run the built `gx lint` against a fixture with a known shellcheck issue (unquoted `$VAR`) and confirm a warn diagnostic; temporarily remove `shellcheck` from `PATH` and confirm the graceful-skip diagnostic with exit 0.
