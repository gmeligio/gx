## 1. Extend the parse model for shell detection

- [ ] 1.1 In `src/domain/workflow_parsed/mod.rs`, add `shell: Option<String>` to `Step` (and its wire struct) with `#[serde(default)]`.
- [ ] 1.2 Add deserialization tests: step with `shell: bash`, `shell: pwsh`, and absent `shell:`.

## 2. shellcheck invocation helper

- [ ] 2.1 Add a small helper under `src/infra/` that: probes for `shellcheck` on `PATH` (once per lint run), and runs `shellcheck` over a given script body returning parsed findings (`SCxxxx`, message, in-script line). Prefer `--format=json1` for stable parsing.
- [ ] 2.2 Helper returns a typed "binary not found" signal distinct from "ran, here are findings" so the rule can degrade gracefully.
- [ ] 2.3 Unit-test the parser against a captured shellcheck JSON sample (no live binary needed in the unit test).

## 3. Add the `run-shellcheck` rule

- [ ] 3.1 Add `RunShellcheck` to `RuleName` (`src/lint/rule.rs`): variant, `Display` → `run-shellcheck`, `FromStr`, roundtrip tests.
- [ ] 3.2 Create `src/lint/run_shellcheck.rs`: `Rule` impl, `default_level()` = `Warn`. In `check`: probe once; if absent, emit a single informational skip diagnostic and return. Otherwise, for each workflow/job/step where `run:` is present and shell is bash/sh (absent `shell:` treated as bash), run the body through the helper and map each finding to a `Diagnostic` scoped `.with_workflow().with_job().with_step()`, message including the `SCxxxx` code.
- [ ] 3.3 Skip steps whose `shell:` is an explicit non-shell (`pwsh`, `python`, `cmd`, ...).
- [ ] 3.4 Register the rule in `src/lint/command.rs`.
- [ ] 3.5 Unit tests for each spec scenario: finding reported at warn (exit 0); escalated to error (exit 1); clean body (no diagnostic); binary-missing skip diagnostic + no failure; non-shell step skipped; `ignore` by workflow/job suppresses. Where a test needs shellcheck output, inject the helper's parsed result rather than depending on a live binary.

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
