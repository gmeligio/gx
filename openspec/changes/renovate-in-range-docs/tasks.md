## 1. Confirm the external claims (already gathered; re-check only if stale)

- [ ] 1.1 Renovate's `rangeStrategy` docs list the managers `update-lockfile` works for, and custom/regex is absent. **This is the load-bearing claim** — if it does not hold, section 2 has no content and the change stops. Record the manager list **verbatim**, since task 2.1 quotes it
- [ ] 1.2 The custom regex manager's capture-group list has no `lockedVersion` and no lock file support
- [ ] 1.3 Renovate's PEP 621 manager matches `pyproject.toml` and lists `uv.lock` under `lockFileNames` — i.e. it does not detect uv by lock presence. Feeds task 2.4
- [ ] 1.4 Renovate's npm manager covers `pnpm-workspace.yaml` catalogs, and lock maintenance is delegated to the ecosystem's own CLI. Note that "native manager maintains the lock" means *the manager owns the lock and drives its CLI* — the point of contrast with gx is ownership, not who executes the write. Task 2.3 must phrase it that way

## 2. Correct and strengthen `docs/renovate.md`

- [ ] 2.1 Rewrite the paragraph at lines 37–41 to cite the `rangeStrategy` option docs as the primary source, quoting the manager list that excludes custom managers; keep the regex capture-group point sourced to the custom manager docs
- [ ] 2.2 Scope the `renovate#19802` demotion to **line 40 only**. The second citation at line 142 backs a different, still-valid argument (Renovate's schema drifts) and must survive
- [ ] 2.3 Add short prose after the two-layer table (lines 11–14) stating that each analogue ecosystem has a native Renovate manager owning its lock and driving the ecosystem CLI to regenerate it, whereas gx is attached by a custom regex manager blind to `gx.lock`. No fourth column, no second table
- [ ] 2.4 Fix the uv analogy at line 146 to describe the PEP 621 manager accurately, without weakening the surrounding argument that a native gx manager is the real endpoint

## 3. Verify

- [ ] 3.1 Confirm no file under `src/` or `tests/` was touched
- [ ] 3.2 Confirm README needs no edit (line 75 already carries the pointer) and record the `docs/demo.tape` judgment
- [ ] 3.3 Confirm each newly added or changed citation resolves, checking the anchor is really present on the page and not just a 200 on the base URL
- [ ] 3.4 Run `mise run test` as a no-regression check and confirm it passes
