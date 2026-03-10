## 1. Replace `module_parent_prefix` with `parent_prefixes`

- [x] 1.1 Add helper `find_tests_rs_includer(tests_file, src_dir) -> Option<PathBuf>` that searches sibling `.rs` files for a `mod tests;` declaration and returns the includer path
- [x] 1.2 Add helper `module_path_segments(file_path, src_dir) -> Vec<String>` that computes the module path segments for any file (handling `mod.rs`, `lib.rs`, `main.rs` stems)
- [x] 1.3 Replace `module_parent_prefix` with `parent_prefixes(file_path, src_dir) -> (Option<String>, Option<String>)` that returns `(file_level_prefix, indented_prefix)` — using the includer resolution for `tests.rs` files

## 2. Update rule 2 check to be depth-aware

- [x] 2.1 Remove the `is_tests_file` skip and the `is_file_level_use` guard from the rule 2 block
- [x] 2.2 Replace with indent-based prefix selection: indent 0 uses `file_level_prefix`, indent 4+ uses `indented_prefix`

## 3. Update architecture-guardrails spec

- [x] 3.1 Replace the "Import path hygiene" requirement in `openspec/specs/architecture-guardrails/spec.md` with the refined version from the delta spec (depth-aware rule 2, new scenarios for inline test modules and `tests.rs` includer resolution)

## 4. Verify

- [x] 4.1 Run `cargo test --test code_health` — all 6 tests pass, zero false positives from inline test modules or `tests.rs` files
- [x] 4.2 Run `cargo check` — no compilation errors from import changes
