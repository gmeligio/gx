## PR 1 — `Commented<T>` refactor (internal, no spec-visible change)

- [ ] 1.1 Change `Step.uses` in `src/domain/workflow_parsed/mod.rs` from `Option<String>` to `Option<Commented<String>>`; update any other readers the compiler flags
- [ ] 1.2 In `src/infra/workflow_scan/scanner.rs`, read the inline comment from `step.uses`'s `Commented` value instead of the regex map
- [ ] 1.3 Delete `USES_WITH_COMMENT_RE`, the `for line in content.lines()` comment-scrape loop, the `HashMap<uses-text, comment>`, and the `comments.get(uses)` lookup
- [ ] 1.4 Remove the now-outdated "saphyr drops comments during parsing" doc comment on `extract_workflow`
- [ ] 1.5 Add a test: two steps with the **same** `uses:` but **different** pinned comments each keep their own comment (regression test for the dup-key bug)
- [ ] 1.6 Verify `cargo build`, `cargo clippy`, and the existing test suite pass with output byte-identical to before (run with `GITHUB_TOKEN=$(gh auth token)` for e2e)

## PR 2 — `Spanned<T>` + line in diagnostics (delivers the spec delta)

- [ ] 2.1 Wrap `Step.uses` as `Option<Spanned<Commented<String>>>`; thread the value/comment through unchanged
- [ ] 2.2 Capture the parsed line in `src/infra/workflow_scan/scanner.rs` and carry it on `Location`/`Located` in `src/domain/workflow_actions.rs`
- [ ] 2.3 Add `line: Option<u32>` and a `with_line` builder to `Diagnostic` in `src/lint/rule.rs`
- [ ] 2.4 Populate `line` in rules that map to a single `uses:` line (sha-mismatch, unpinned, stale-comment); leave manifest/whole-file rules with `line = None`
- [ ] 2.5 Render `path:line:` in `Line::LintDiag` in `src/output/lines.rs` when `line` is `Some`; keep `path:` rendering when `None`
- [ ] 2.6 Add tests: a located violation prints `file:line:`; a manifest-level violation prints `file:` with no line (no regression)
- [ ] 2.7 Verify `cargo build`, `cargo clippy`, and the full test suite pass (e2e with `GITHUB_TOKEN`)
