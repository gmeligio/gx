## 1. mise tasks (source of truth)

> **Layout note (D7):** mise 2026.6.0 has no "primary file in a task dir" convention — a file at `.config/mise/tasks/format/check` resolves to task `format:check`, but the directory has no implicit bare `format` task. To get a `:check` variant the bare task must move *into* the directory too. So `format` becomes `format/format` (task `format:format`) and `clippy` becomes `clippy/clippy` (task `clippy:clippy`). The bare `format`/`clippy` names cease to exist — every reference migrates to the namespaced name (see §1.7, §2, §3, §5).

- [x] 1.1 Convert `.config/mise/tasks/format` (file) into a directory: move it to `.config/mise/tasks/format/format` and set its body to `cargo fmt --all` (mutating; was `cargo fmt`). Task name becomes `format:format`.
- [x] 1.2 Add `.config/mise/tasks/format/check` running `cargo fmt --all --check` (non-mutating, CI verify). Task name `format:check`.
- [x] 1.3 Convert `.config/mise/tasks/clippy` (file) into a directory: move it to `.config/mise/tasks/clippy/clippy`, keeping its body `cargo clippy --locked --tests --fix --allow-dirty` (mutating). Task name becomes `clippy:clippy`.
- [x] 1.4 Add `.config/mise/tasks/clippy/check` running `cargo clippy --locked --tests -- -D warnings` (no `--fix`, CI verify). Task name `clippy:check`. (Deliberately carries NO `lint:size` dependency — CI's Integration Tests job already runs the `code_health` size test, and a per-check CI job should verify one thing; the mutating `clippy:clippy` keeps `lint:size` per D6.)
- [x] 1.5 Add `.config/mise/tasks/check` (file) running `cargo check --locked`. Task name `check` (no `:check` variant needed — `cargo check` is already non-mutating).
- [x] 1.6 In the moved `clippy:clippy` task, remove `format` from `#MISE depends=["lint:size", "format"]` so it becomes `#MISE depends=["lint:size"]` (D6: avoid double-format; keep `lint:size`). Note the dependency on `lint:size` is unaffected by the rename.
- [x] 1.7 Grep for any remaining bare-name task references and migrate them to `format:format` / `clippy:clippy` (e.g. other tasks' `depends`, the `demo`/`setup`/`upgrade` tasks if they chain format/clippy). Confirm no `depends`/`mise run` still names bare `format` or `clippy`. (Only straggler: `build.yml:41` `mise run clippy` — handled in §2.2.)
- [x] 1.8 Verify each task runs and the bare names are gone: `mise tasks ls` shows `format:format`, `format:check`, `clippy:clippy`, `clippy:check`, `check` (and NO bare `format`/`clippy`); then run `mise run format:format`, `mise run format:check`, `mise run clippy:clippy`, `mise run clippy:check`, `mise run check`

## 2. CI workflow (delegate every job to mise)

- [x] 2.1 In `.github/workflows/build.yml`, change the Format job's `cargo fmt --check` to `mise run format:check` (also added the missing `mise-action` step the Format job needed to resolve mise tasks)
- [x] 2.2 Change the Clippy job's `mise run clippy` to `mise run clippy:check` (CI must not auto-`--fix`; note the old `mise run clippy` would now be `mise run clippy:clippy` anyway, so this swap also fixes the stale bare name)
- [x] 2.3 Change the Check job's `cargo check --locked` to `mise run check`
- [x] 2.4 Confirm no `run:` step in `build.yml` invokes `cargo`/`clippy`/`rustfmt` directly — every check is `mise run <task>` (drift guarantee). Verified: all 7 `run:` steps are `mise run <task>`.
- [x] 2.5 Confirm the Format/Clippy jobs still have the toolchain setup they need (`setup-rust-toolchain` before `mise-action`) after the command swap. Verified: both jobs keep `setup-rust-toolchain` → `mise-action` → `mise run`.

## 3. Pre-commit hooks (mutate + re-stage via mise)

- [ ] 3.1 In `.pre-commit-config.yaml`, change `cargo-fmt` hook entry to run the mutating `mise run format:format` then `git add -u` (e.g. `entry: bash -c 'mise run format:format && git add -u'`)
- [ ] 3.2 Change `cargo-linter` hook entry to run the mutating `mise run clippy:clippy` then `git add -u`
- [ ] 3.3 Change `gx-lockfile` hook entry from `gx tidy` to `mise run tidy` (fixes stale installed-binary bug); add `git add -u` if `tidy` can modify tracked files
- [ ] 3.4 Leave `cargo-lockfile` and `mise-lockfile` hooks inline (no task/CI counterpart) and the `cargo-deny` pre-push hook unchanged (already delegates)

## 4. Verification

- [ ] 4.1 Stage a deliberately misformatted `.rs` change, `git commit`, and confirm the commit succeeds in one shot with formatting applied and re-staged (D3 zero-friction)
- [ ] 4.2 Confirm `mise run format:check` and `mise run clippy:check` are non-mutating: clean tree before and after
- [ ] 4.3 Run `mise run check`, `mise run clippy:clippy`, `mise run format:format` locally and confirm success
- [ ] 4.4 Push the branch and confirm all `build.yml` jobs pass (Format, Clippy, Check, Unit/Integration/E2E Tests, Deny)
- [ ] 4.5 `AGENTS.md` line 2 is generic (`mise run <task>`, no specific name) so it needs no edit; confirm no other doc, README, or `docs/demo.tape` references the now-removed bare `format`/`clippy` task names (grep already done in §1.7) — update any that do
