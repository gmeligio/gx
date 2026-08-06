## 1. Baseline

- [x] 1.1 Run `mise run test` and `mise run integ` on the untouched tree and record that both are green, so any later failure is attributable to this change

## 2. Rename the `Line` identifier field

- [x] 2.1 In `src/output/lines.rs`, rename `action: String` to `id: String` on `Line::Upgraded`, `Added`, `Removed`, `Changed`, `Skipped`, updating each variant's doc comment to describe the identifier rather than "an action"
- [x] 2.2 Update the matching `format_line` arms to bind `id`, leaving every format string and column width (`{id:<30}`) byte-for-byte as it renders today
- [x] 2.3 Update the `format_line_*` unit tests in `lines.rs` to the new field name without changing a single asserted string

## 3. Update `Line` producers

- [x] 3.1 In `src/upgrade/report.rs`, construct `OutputLine::Upgraded`/`Skipped` with `id:`, leaving `UpgradeEntry.action` and `SkippedEntry.action` (the `Serialize` fields) untouched
- [x] 3.2 In `src/tidy/report.rs`, construct `OutputLine::Removed`/`Added`/`Upgraded` with `id:`
- [x] 3.3 Build, then confirm no remaining `Line { action: ... }` construction site exists anywhere in `src/` or `tests/`, and that `src/init/report.rs` and `src/lint/report.rs` needed no production change (neither constructs a renamed variant). Note that `Line::Changed` has no construction site at all — it is renamed for consistency, not deleted; file a follow-up issue rather than removing it here

## 4. Empty the noun out of rule messages

- [ ] 4.1 `src/lint/unpinned.rs`: drop the leading `action ` from the message so it reads `{id} uses tag reference {ref} instead of SHA pin`
- [ ] 4.2 `src/lint/sha_mismatch.rs`: drop the leading `action ` so it reads `{id} SHA {sha} not found in lock file`
- [ ] 4.3 `src/lint/stale_comment.rs`: drop the leading `action ` so it reads `{id} version {v} has stale comment (...)`
- [ ] 4.4 `src/lint/unsynced_manifest.rs`: drop the leading `action ` from both messages (used-not-declared and declared-not-used) — this rule accounts for two of the five changed strings
- [ ] 4.5 Re-grep `src/lint/` for a kind-noun prefix in any remaining message to confirm these five (across four rules, of gx's thirteen) were the only ones

## 5. Guard against reintroduction

- [ ] 5.1 Add `rendered_diagnostics_carry_no_kind_noun` to `src/lint/report.rs` tests — the `Diagnostic` → `Line::LintDiag` chokepoint. Invoke at minimum the four previously-offending rules (`unpinned`, `sha-mismatch`, `stale-comment`, `unsynced-manifest`) over fixtures, render the `Report`, and assert no resulting message opens with a kind-noun (`action `, `workflow `, `component `). Every asserted string must come from the rule's own `format!`; never assert on a message literal the test wrote. Task 5.3 is what proves this property holds — do not treat 5.1 as self-checking
- [ ] 5.2 Add a rule-side assertion to `unpinned.rs` mirroring its existing `message_does_not_embed_workflow_path`, so a failure names the offending rule
- [ ] 5.3 Falsify the guard: temporarily restore the `action ` prefix in a rule and confirm 5.1 goes red. If it stays green the test is inert and must be rewritten before proceeding. Restore the fix afterward
- [ ] 5.4 Keep the guard within `src/lint/`'s file budget — it is at 8/8 `.rs` files, so this test goes in the existing `report.rs`, never a new file

## 6. Prove output is unchanged

- [ ] 6.1 Run `mise run test`; confirm the exact-string summary tests (`render_upgrade_with_upgrades`, `render_tidy_with_changes`, `render_init_with_actions`, `render_lint_with_violations`, `render_upgrade_up_to_date`) pass unmodified
- [ ] 6.2 Confirm the `--json` tests (`to_json_uses_resolved_versions_and_compare`, `to_json_omits_compare_when_absent`, `to_json_up_to_date_has_empty_upgrades`) pass unmodified, proving the serialized contract is intact
- [ ] 6.3 Run `mise run integ` and confirm green
- [ ] 6.4 Review the full diff and confirm the only changed user-visible strings are the five lint messages named in section 4

## 7. Spec and close-out

- [ ] 7.1 Confirm the `command-output` delta spec matches what shipped; adjust if implementation diverged
- [ ] 7.2 Commit with a Conventional Commits title (`refactor(output): ...`), using `--no-gpg-sign` if signing is refused
