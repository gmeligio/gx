## 1. Baseline — capture the invariants before touching anything

- [ ] 1.1 Record the current `src/lint/` direct `.rs` file count via
      `ls src/lint/*.rs | wc -l` (expected 8) for the before/after report
- [ ] 1.2 Capture golden output: run `mise run integ` and save the rendered lint
      output for the fixtures to the scratchpad, to diff against after the move
- [ ] 1.3 Confirm the pre-change name mapping: for all 13 variants, record the
      `Display` string and the `rename_all = "kebab-case"` serde string, and
      verify they are pairwise identical (design D2's byte-compatibility claim)

## 2. Collapse the name lists into one (`rule_ids!`)

- [ ] 2.1 Write the `rule_ids!` declarative macro generating enum, `as_str`,
      `ALL`, `Display`, and `FromStr` from a single `Variant => "name"` list
- [ ] 2.2 Add `Serialize`/`Deserialize` impls to the macro that delegate to
      `as_str` / `FromStr`, and drop `#[serde(rename_all = "kebab-case")]`
- [ ] 2.3 Re-express `RuleName`'s 13 variants as one `rule_ids!` invocation,
      deleting the hand-written `Display` and `FromStr` blocks
- [ ] 2.4 Add the per-variant agreement test: iterate `RuleName::ALL` and assert
      serde form == `as_str` == `Display` == `FromStr` round trip
- [ ] 2.5 Replace the hand-written `rule_name_from_str_valid` 13-assert test with
      one driven by `ALL`, so adding a rule needs no test edit

## 3. Extract the shared diagnostics home

- [ ] 3.1 Create `src/diagnostic/` with `mod.rs`, `record.rs`, `report.rs`; add
      `pub mod diagnostic;` to `src/lib.rs`
- [ ] 3.2 Move `Diagnostic` + builders into `record.rs` as `Diagnostic<Id>`,
      generic over rule identity (design D4)
- [ ] 3.3 Move the three ignore matchers and `workflow_matches` into `record.rs`
- [ ] 3.4 Move counting, `exit_code`, and summary pluralization into
      `diagnostic/report.rs` as `Report<Id>`
- [ ] 3.5 Move `Level`, `IgnoreTarget`, `Rule` config types out of `src/config.rs`
      into the shared home (or re-export) so `config.rs` no longer needs
      `crate::lint::`

## 4. Repoint the consumers

- [ ] 4.1 Rename `src/lint/rule.rs` residue to `src/lint/identity.rs`, holding
      `RuleName` (the `rule_ids!` call), the `Rule` trait, `Context`, and runner
      wrappers; delete `src/lint/report.rs`
- [ ] 4.2 Add `lint::Diagnostic` / `lint::Report` type aliases over the generic
      types; keep `gx::lint::{Diagnostic, RuleName, Context, Rule}` re-exported so
      `tests/` compiles unchanged
- [ ] 4.3 Update the 13 rule files and `command.rs` to the new import paths
- [ ] 4.4 Repoint `src/config.rs` and `src/infra/manifest/convert.rs` off
      `crate::lint::RuleName`; verify no non-command module imports `crate::lint::`

## 5. Spec-driven tests

- [ ] 5.1 Add the 13-name config parse test generated from `RuleName::ALL`
      (replaces the drifted 10-name enumeration)
- [ ] 5.2 Add the previously missing test for the unrecognized-rule-name scenario:
      a typo'd `[lint.rules]` key fails parsing and the error names the key
- [ ] 5.3 Update `openspec/specs/lint-command/spec.md`'s "All valid rule names
      accepted" scenario from 10 names to 13 at archive time (delta spec already
      carries the corrected text)

## 6. Verify

- [ ] 6.1 Mutation-test the guard: break one variant's `as_str` string, confirm
      the agreement test AND `mise run integ` both go red, then restore
- [ ] 6.2 Mutation-test the config surface: make one rule's config name differ
      from its reported name, confirm a test catches it, then restore
- [ ] 6.3 `mise run test` passes with no budget number in `tests/code_health.rs`
      raised (confirm via `git diff tests/code_health.rs` being empty)
- [ ] 6.4 `mise run integ` passes and its output diffs clean against the 1.2
      golden capture — the byte-identical proof
- [ ] 6.5 Report `src/lint/` file count after (target: 7, at least one free slot)
      and confirm `src/diagnostic/` is within the 8-file budget
