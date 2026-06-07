## 1. mise tasks (source of truth)

- [ ] 1.1 Update `.config/mise/tasks/format` to `cargo fmt --all` (mutating; was `cargo fmt`)
- [ ] 1.2 Add `.config/mise/tasks/format:check` running `cargo fmt --all --check` (non-mutating, CI verify)
- [ ] 1.3 Add `.config/mise/tasks/clippy:check` running `cargo clippy --locked --tests -- -D warnings` (no `--fix`, CI verify)
- [ ] 1.4 Add `.config/mise/tasks/check` running `cargo check --locked`
- [ ] 1.5 Remove `format` from the `clippy` task's `#MISE depends=["lint:size", "format"]` so it becomes `depends=["lint:size"]` (D6: avoid double-format; keep `lint:size`)
- [ ] 1.6 Verify each task runs: `mise run format`, `mise run format:check`, `mise run clippy`, `mise run clippy:check`, `mise run check`

## 2. CI workflow (delegate every job to mise)

- [ ] 2.1 In `.github/workflows/build.yml`, change the Format job's `cargo fmt --check` to `mise run format:check`
- [ ] 2.2 Change the Clippy job's `mise run clippy` to `mise run clippy:check` (CI must not auto-`--fix`)
- [ ] 2.3 Change the Check job's `cargo check --locked` to `mise run check`
- [ ] 2.4 Confirm no `run:` step in `build.yml` invokes `cargo`/`clippy`/`rustfmt` directly — every check is `mise run <task>` (drift guarantee)
- [ ] 2.5 Confirm the Format/Clippy jobs still have the toolchain setup they need (`setup-rust-toolchain` before `mise-action`) after the command swap

## 3. Pre-commit hooks (mutate + re-stage via mise)

- [ ] 3.1 In `.pre-commit-config.yaml`, change `cargo-fmt` hook entry to run `mise run format` then `git add -u` (e.g. `entry: bash -c 'mise run format && git add -u'`)
- [ ] 3.2 Change `cargo-linter` hook entry to run `mise run clippy` then `git add -u`
- [ ] 3.3 Change `gx-lockfile` hook entry from `gx tidy` to `mise run tidy` (fixes stale installed-binary bug); add `git add -u` if `tidy` can modify tracked files
- [ ] 3.4 Leave `cargo-lockfile` and `mise-lockfile` hooks inline (no task/CI counterpart) and the `cargo-deny` pre-push hook unchanged (already delegates)

## 4. Verification

- [ ] 4.1 Stage a deliberately misformatted `.rs` change, `git commit`, and confirm the commit succeeds in one shot with formatting applied and re-staged (D3 zero-friction)
- [ ] 4.2 Confirm `mise run format:check` and `mise run clippy:check` are non-mutating: clean tree before and after
- [ ] 4.3 Run `mise run check`, `mise run clippy`, `mise run format` locally and confirm success
- [ ] 4.4 Push the branch and confirm all `build.yml` jobs pass (Format, Clippy, Check, Unit/Integration/E2E Tests, Deny)
- [ ] 4.5 Update `AGENTS.md`/docs only if any task name referenced there changed (otherwise no doc change needed)
