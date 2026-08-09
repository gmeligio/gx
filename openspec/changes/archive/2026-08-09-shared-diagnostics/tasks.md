## 1. Baseline — capture the invariants before touching anything

- [x] 1.1 Record the current `src/lint/` direct `.rs` file count via
      `ls src/lint/*.rs | wc -l` (expected 8) for the before/after report
      → confirmed 8
- [x] 1.2 Capture golden output: run `mise run integ` and save the rendered lint
      output for the fixtures to the scratchpad, to diff against after the move
      → `golden-before.txt`: 9 diagnostics across 6 rule families + summary,
      exit 1; `mise run integ` green
- [x] 1.3 Confirm the pre-change name mapping: for all 13 variants, record the
      `Display` string and the `rename_all = "kebab-case"` serde string, and
      verify they are pairwise identical (design D2's byte-compatibility claim)
      → all 13 Display strings are exactly kebab-case of the variant name

## 2. Collapse the name lists into one (`rule_ids!`)

- [x] 2.1 Write the `rule_ids!` declarative macro generating enum, `as_str`,
      `ALL`, `Display`, and `FromStr` from a single `Variant => "name"` list
- [x] 2.2 Add `Serialize`/`Deserialize` impls to the macro that delegate to
      `as_str` / `FromStr`, and drop `#[serde(rename_all = "kebab-case")]`
- [x] 2.3 Re-express `RuleName`'s 13 variants as one `rule_ids!` invocation,
      deleting the hand-written `Display` and `FromStr` blocks
- [x] 2.4 Add the per-variant agreement test: iterate `RuleName::ALL` and assert
      serde form == `as_str` == `Display` == `FromStr` round trip
- [x] 2.5 Replace the hand-written `rule_name_from_str_valid` 13-assert test with
      one driven by `ALL`, so adding a rule needs no test edit
- [x] 2.6 Confirm the map-key write path: round-trip a populated `[lint.rules]`
      table through serialize→deserialize and assert the emitted keys are
      unchanged (design D2 map-key note)

## 3. Extract the shared diagnostics home

- [x] 3.1 Create `src/diagnostic/` with `mod.rs`, `record.rs`, `report.rs`; add
      `pub mod diagnostic;` to `src/lib.rs`
- [x] 3.2 Move `Diagnostic` + builders into `record.rs`, naming `RuleName`
      concretely (design D4)
- [x] 3.3 Move the three ignore matchers and `workflow_matches` into `record.rs`
- [x] 3.4 Move counting, `exit_code`, and summary pluralization into
      `diagnostic/report.rs` as `Report`
- [x] 3.5 Leave `Level`, `IgnoreTarget`, and `Rule` defined in `src/config.rs` —
      they are already generic and `lint/` imports them, which is the correct
      direction. The `config → lint` edge comes solely from `RuleName`
      (`config.rs:69,76`) and is severed by 4.4, not by moving these.

## 4. Repoint the consumers

- [x] 4.1 Trim `src/lint/rule.rs` to its residue — `RuleName` (the `rule_ids!`
      call), the `Rule` trait, `Context`, and runner wrappers — keeping the
      filename; delete `src/lint/report.rs` (8 → 7 files)
- [x] 4.2 Re-export `lint::Diagnostic` / `lint::Report` from `crate::diagnostic`;
      keep `gx::lint::{Diagnostic, RuleName, Context, Rule}` re-exported so
      `tests/` compiles unchanged
- [x] 4.3 Update the 13 rule files and `command.rs` to the new import paths
- [x] 4.4 Repoint `src/config.rs` and `src/infra/manifest/convert.rs` off
      `crate::lint::RuleName`; verify no non-command module imports `crate::lint::`

## 5. Spec-driven tests

- [x] 5.1 Add the 13-name config parse test generated from `RuleName::ALL`
      (replaces the drifted 10-name enumeration)
- [x] 5.2 Add the previously missing test for the unrecognized-rule-name scenario:
      a typo'd `[lint.rules]` key fails parsing and the error names the key
- [x] 5.3 Add a test driven by `RuleName::ALL` asserting every implemented rule has
      a `default_level`, covering the corrected 13-rule zero-config default set
      (`dangling-reference` = error, `invalid-expression` = error,
      `run-shellcheck` = warn were missing from the spec's list of 10)

## 6. Verify

- [x] 6.1 Mutation-test the guard: break one variant's `as_str` string, confirm
      the guards go red, then restore.
      → Renaming `Unpinned => "unpinned-MUTANT"` turned 3 config-parsing tests red
      (`config::tests::lint_config_parses_multiple_rules`,
      `infra::manifest::parse::tests::parse_lint_config_{with_rules,ignore_targets}`)
      AND changed the rendered CLI output to `unpinned-MUTANT: …` — one edit moved
      both the config surface and the output together, which is the single-list
      guarantee. **Correction to the planned claim:** `mise run integ` stayed green,
      because `integ_lint.rs` compares `RuleName` enum values rather than strings.
      The unit-level agreement test is therefore the real guard for this contract,
      not integ — which is exactly why it was added.
- [x] 6.2 Mutation-test the "rule added later is auto-covered" property: adding a
      14th variant failed ONLY `all_covers_every_rule_and_names_are_unique` (the
      count guard) and `every_rule_has_a_documented_default_level` (the defaults
      table) — the four contract tests covered the new rule with no edits. That is
      the anti-conflict property #130/#131/#132 need. Restored.
- [x] 6.3 `mise run test` passes with no budget number in `tests/code_health.rs`
      raised (confirm via `git diff tests/code_health.rs` being empty)
- [x] 6.4 `mise run integ` passes and its output diffs clean against the 1.2
      golden capture — the byte-identical proof
- [x] 6.5 Report `src/lint/` file count after (target: 7, at least one free slot)
      and confirm `src/diagnostic/` is within the 8-file budget
      → `src/lint/` 8 → 7 (one free slot); `src/diagnostic/` 5 of 8
      (`mod.rs`, `identity.rs`, `record.rs`, `report.rs`, `rule_name.rs`);
      no module outside `src/lint/` imports `crate::lint` any more
