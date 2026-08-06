## 1. Re-verify the external claims before editing

- [ ] 1.1 Confirm Renovate's `rangeStrategy` docs still list a closed set of managers for `update-lockfile` and that custom/regex is absent from it
- [ ] 1.2 Confirm the custom regex manager's capture-group list still has no `lockedVersion` and no lock file support
- [ ] 1.3 Confirm how Renovate's PEP 621 manager relates to `uv.lock` (file patterns vs `lockFileNames`), to settle the uv detection claim inherited from issue #109
- [ ] 1.4 Confirm Renovate's npm manager covers `pnpm-workspace.yaml` catalogs and delegates lock maintenance to the package manager CLI

## 2. Correct and strengthen `docs/renovate.md`

- [ ] 2.1 In the "what Renovate cannot do" section, cite the `rangeStrategy` manager list as the primary source and quote the closed list; keep the regex capture-group point, sourced to the custom manager docs
- [ ] 2.2 Demote `renovate#19802` from load-bearing evidence to at most background, since the option docs now state the limitation directly
- [ ] 2.3 Add the npm / uv / pnpm comparison table required by issue #109's acceptance criterion, showing each ecosystem's manifest, lock, and the native manager that closes the gap
- [ ] 2.4 State the correction to the uv claim: detection is via `pyproject.toml`, with `uv.lock` named as a lock file the manager maintains — not detection by lock presence
- [ ] 2.5 Keep the plain statement "Renovate catches majors; in-range advancement is `gx upgrade`'s job" prominent

## 3. Reconcile the workflow-template references

- [ ] 3.1 Confirm whether `docs/gx-upgrade.yml` and `docs/upgrade-workflow.md` exist in this worktree
- [ ] 3.2 If absent, phrase the remedy around `gx upgrade --json` (which ships today) and reference the workflow template only as its documented destination — never link a file that is not in the tree

## 4. Verify

- [ ] 4.1 Confirm no file under `src/` or `tests/` was touched
- [ ] 4.2 Confirm README needs no edit (line 75 already carries the pointer) and record the `docs/demo.tape` judgment
- [ ] 4.3 Check every external URL cited in the page resolves
- [ ] 4.4 Run `mise run test` as a no-regression check and confirm it passes
