## 1. mise tasks (promote `test`, add `test:unit` leaf)

- [x] 1.1 Convert `.config/mise/tasks/test` (file) into a directory: move the current body (`cargo test --locked --lib`) to `.config/mise/tasks/test/unit`. Task name becomes `test:unit`; keep the `#MISE description` describing the unit suite.
- [x] 1.2 Add the `test` gate as a `[tasks.test]` block in `.config/mise.toml` (D4 — a file in `test/` cannot yield the bare `test` name): `depends = ["check", "format:check", "clippy:check", "lint:size", "test:unit"]`, a `description = "Local gate: typecheck, format, lint, size budgets, unit tests"`, and a `# keep in sync with build.yml PR-check jobs` comment. Task name `test`.
- [x] 1.3 Verify naming: `mise tasks ls` shows `test` (gate) and `test:unit` (leaf); `mise run test` resolves to the gate (not an error).

## 2. Repoint existing consumers of `test`

- [x] 2.1 In `.config/mise/tasks/test-all`, change `#MISE depends=["test", "integ", "e2e"]` to `#MISE depends=["test:unit", "integ", "e2e"]` (it wants the unit suite, not the gate).
- [x] 2.2 In `.github/workflows/build.yml`, change the Unit Tests job's `mise run test` to `mise run test:unit`.
- [x] 2.3 Grep to confirm no other reference to bare `mise run test` or `depends=[..."test"...]` remains (only `build.yml` and `test-all` existed pre-change). Verified: none remain.

## 3. Verification

- [ ] 3.1 Run `mise run test:unit` and confirm it runs exactly `cargo test --locked --lib` (matches the old `test` behavior).
- [ ] 3.2 Run `mise run test` and confirm it executes check + format:check + clippy:check + lint:size + test:unit (members run in parallel; each labeled), exiting 0 on a clean tree.
- [ ] 3.3 Deliberately break one member (e.g. introduce a clippy warning or a misformat), run `mise run test`, and confirm the gate fails and the failing member is named; then revert.
- [ ] 3.4 Confirm `lint:size` runs once under the gate (not double-counted): `test:unit` is `--lib`, `lint:size` is `--test code_health` — disjoint.
- [ ] 3.5 Run `mise run test-all` and confirm it still resolves (`test:unit`, `integ`, `e2e`).
- [ ] 3.6 Push the branch and confirm all `build.yml` jobs pass — in particular the Unit Tests job now running `mise run test:unit`.
- [ ] 3.7 Manual drift check (D3, no automated guard): confirm every PR-check job in `build.yml` (Check, Format, Clippy, Unit Tests) has a corresponding member in the gate's `depends`. Note any intentional differences (the gate uses `test:unit`; the gate excludes the slow `integ`/`e2e`/`deny` by design).
