# Code Health Gates — Tasks

## Phase 1: Helpers

- [x] Add `count_non_test_lines()` helper — counts file lines excluding `#[cfg(test)]` blocks (simplified: first `#[cfg(test)]` to EOF)
- [x] Add `ModRsScanner` struct with `is_structural_line(&mut self, line: &str) -> bool` — classifies mod.rs lines as structural (mod/use/comments/attributes/blanks), with stateful tracking of `in_use_block` and `use_brace_depth` for multi-line `use {}` blocks

## Phase 2: Gates

- [x] Re-measure `logic_line_budget` max — run `count_non_test_lines()` against `src/` and compare with proposal value (438). If the measured max exceeds the proposed budget (440) by more than 10 lines, flag for proposal amendment before proceeding
- [x] Re-measure `mod_rs_reexports_only` per-file max — run the `ModRsScanner` against all `mod.rs` files, compare with proposal value (354). Confirm no `mod.rs` contains block comments that trigger the `/* ... */` miscount limitation. If the measured max exceeds the proposed budget (360) by more than 10 lines, flag for proposal amendment
- [x] Re-measure generic file name count — scan `src/` for denylist matches, compare with proposal value (1). If count exceeds the proposed budget (1), flag for proposal amendment
- [x] Add `logic_line_budget` gate (Task 1.5) — budget 440, target 300. Skip standalone `tests.rs` files. Include secondary assertion validating the "all production code precedes `#[cfg(test)]`" invariant
- [x] Add `mod_rs_reexports_only` gate (Task 1.6) — budget 360, target 0. Use `count_non_test_lines()` to determine the non-test line range, then classify with `ModRsScanner`. Report all mod.rs files with logic > 0 for visibility
- [x] Add `no_generic_file_names` gate (Task 1.7) — budget 1, target 0. Denylist: types.rs, utils.rs, helpers.rs, common.rs, misc.rs, consts.rs, constants.rs

## Phase 3: Tests

- [x] Add inline tests for `count_non_test_lines()` — cover: no test block, inline test block at EOF, `#[cfg(test)] mod tests;` declaration, empty file, `#[cfg(test)]` appearing inside a string literal or doc comment or `macro_rules!` body (documents known limitations of simplified algorithm)
- [x] Add inline tests for `ModRsScanner::is_structural_line()` prefix classification — cover: mod/use/comment/attribute/blank lines
- [x] Add inline tests for `ModRsScanner::is_structural_line()` stateful multi-line `use {}` block tracking — cover: single-line use, multi-line use with nested braces, interleaved logic lines

## Phase 4: Verify

- [x] Run `mise run test` to verify all gates pass on current codebase — if a gate fails, adjust the budget (the gate is wrong, not the code)
