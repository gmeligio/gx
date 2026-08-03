## 1. Unblock main

- [ ] 1.1 Run `mise install` and commit the resulting `.config/mise.lock` so the tree is a fixed point for the current mise binary (migration plan step 1 — without this the new job fails on its own PR)
- [ ] 1.2 Record in the commit body which fields changed, so the next occurrence of this class has a precedent to compare against (#66 and #112 are the prior two)

## 2. Lock task pair

- [ ] 2.1 Create `.config/mise/tasks/lock/_default` running `mise install`, with `#MISE description="Sync .config/mise.lock with the installed tools"` and a `Don't rename` comment matching the convention in `clippy/_default` and `format/_default`
- [ ] 2.2 Create `.config/mise/tasks/lock/check` running the mutating task then `git diff --exit-code -- .config/mise.lock`, with `#MISE description="Verify .config/mise.lock is unchanged by an install (CI)"`
- [ ] 2.3 Comment in `lock/check` why it cannot use `--locked` (the `core:rust` / `locked = false` catch-22 documented in `.config/mise.toml`), so the next reader does not "simplify" it into a broken form
- [ ] 2.4 Verify `mise tasks` lists both `lock` and `lock:check` with their descriptions

## 3. Local hook

- [ ] 3.1 Change the `mise-lockfile` hook entry in `.pre-commit-config.yaml` to `bash -c 'mise run lock && git add -u'`
- [ ] 3.2 Update that hook's description: it now re-stages rather than blocking, and it delegates to the mise task rather than calling `mise install` inline
- [ ] 3.3 Confirm the hook no longer invokes `mise install` directly, satisfying the "no inline check commands" requirement

## 4. CI and local gate

- [ ] 4.1 Add a `lockfile` job to `.github/workflows/build.yml` named `Lockfile`, following the same three-step shape as the other 8 jobs, with `run: mise run lock:check`
- [ ] 4.2 Add `lock:check` to the `depends` list in `.config/mise/tasks/test/_default`, keeping it in sync with the PR-check jobs as its comment requires
- [ ] 4.3 Confirm `.github/gx.lock` still matches the workflows after editing `build.yml` (the `gx-lockfile` hook should handle this on commit; verify it did)

## 5. Verify behavior

- [ ] 5.1 Drift detection: inject a line into `.config/mise.lock`, run `mise run lock:check`, confirm non-zero exit and that the injected line appears in the printed diff
- [ ] 5.2 Clean pass: run `mise run lock:check` twice against an untouched tree, confirm exit 0 both times (proves the task is not itself a writer that dirties the file)
- [ ] 5.3 Hook auto-fix: stage an unrelated file, inject drift, commit; confirm prek reports `Failed` with "files were modified by this hook", the lockfile is reverted and staged, and an immediate re-commit succeeds
- [ ] 5.4 Gate membership: confirm `mise run test` resolves `lock:check`, and that a drifted lockfile fails the gate
- [ ] 5.5 CI: confirm the `Lockfile` job appears and passes on this change's own PR

## 6. Update specs

- [ ] 6.1 Apply the `lockfile-integrity` delta to `openspec/specs/lockfile-integrity/spec.md`
- [ ] 6.2 Apply the `task-execution-consistency` delta to `openspec/specs/task-execution-consistency/spec.md`
- [ ] 6.3 Add a note to `openspec/changes/archive/2026-06-06-add-prek-lockfile-hooks/design.md` Decision 5 recording that it was reversed, and why (drift originating in CI, not from a bypassed hook), so the reversal is discoverable from where the original decision is documented
