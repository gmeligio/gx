## 1. Unblock the release (immediate, standalone)

- [ ] 1.1 Run `mise install` and confirm the only change to `.config/mise.lock` is the `@generated` header (`mise.jdx.dev` → `mise.en.dev`)
- [ ] 1.2 Commit the regenerated `.config/mise.lock` so the next `release-plz` run sees a clean tree

## 2. Cargo.lock — native `--locked` verification

- [ ] 2.1 Add `--locked` to `cargo check` in `.github/workflows/build.yml` (the `check` job)
- [ ] 2.2 Add `--locked` to the cargo invocation in `.config/mise/tasks/clippy`
- [ ] 2.3 Add `--locked` to the cargo invocation in `.config/mise/tasks/test`
- [ ] 2.4 Verify locally that `cargo check --locked` and `mise run clippy`/`mise run test` pass with the committed `Cargo.lock`

## 3. gx.lock — `gx tidy` verification

- [ ] 3.1 Add a CI step (PR job in `build.yml`) that installs gx via the existing `mise run install` task
- [ ] 3.2 Add a `gx tidy` step after install that fails the job if `.github/gx.lock` / manifest no longer match the workflows
- [ ] 3.3 Verify locally that `gx tidy` is clean against the current workflows

## 4. mise.lock — drift detection (no `--locked`)

- [ ] 4.1 In `build.yml`, after mise-action's (unlocked) install, add `git diff --exit-code .config/mise.lock`
- [ ] 4.2 Make the failure actionable: print a message instructing the maintainer to run `mise install` and commit the regenerated lock
- [ ] 4.3 Confirm the step does NOT use `MISE_LOCKED` / `mise install --locked` (would trigger the `core:rust` catch-22 on a cold runner)

## 5. Verification

- [ ] 5.1 Open a PR and confirm all guard steps are green with committed lockfiles
- [ ] 5.2 Negative test: on a throwaway branch, revert the `.config/mise.lock` header and confirm the mise diff step fails the PR with the intended message
- [ ] 5.3 Negative test: introduce a deliberately stale `Cargo.lock` and confirm `cargo --locked` fails the PR
- [ ] 5.4 Confirm `release-plz` on `main` is unblocked after task group 1 lands
