## 1. Format

- [ ] 1.1 Add `src/infra/lock/actions_lock/` with a YAML reader (via `serde-saphyr`) for schema v0.0.1–v0.0.3: closed key sets, required fields, `sha1-`/`sha256-` digest check, key/ref and SHA-key/commit agreement, acyclic `uses`, safe workflow path keys. Newer version → typed error; parse failure → error with line/column.
- [ ] 1.2 Add the writer: header, sorted sections, one quoting function (single quotes, `'` doubled) for every user-supplied scalar, 4-space indent, empty sections omitted. Preserve an existing header from another producer.
- [ ] 1.3 Golden test: copy `gh-actions-lock`'s own `.github/workflows/actions.lock` as a fixture; read → write is byte-identical (after rewriting `v0.0.2` to `v0.0.3`).
- [ ] 1.4 Tests for YAML-ambiguous refs (`1.0`, `on`, `no`, a ref containing `'`), future version, unknown key, corrupt file.

## 2. Domain

- [ ] 2.1 Replace `Lock`'s `(Spec → LockEntry)` map with a pin-keyed dependency map plus a workflow → pins map; keep `diff`, `retain`, `entries` working for callers.
- [ ] 2.2 `Pin`: canonical `owner/repo@ref` (lowercase owner/repo, subpath dropped).
- [ ] 2.3 Derive specifier → version from the workflow's pin `ref`; derive ref kind (version → tag; otherwise needs lookup).
- [ ] 2.4 Workflow inclusion rule and closure flattening; warning for skipped workflows.

## 3. Resolution

- [ ] 3.1 Extend `VersionRegistry` (and the caching/retrying decorators and fakes) with `repo_ids(owner, repo)` and `action_definition(repo, path, sha)`.
- [ ] 3.2 GitHub adapter: `GET /repos/{owner}/{repo}` for IDs; contents API at the commit for `action.yml`/`action.yaml`.
- [ ] 3.3 Transitive resolver: classify child `uses:`, resolve at their own ref, detect cycles, no manifest involvement.

## 4. Commands

- [ ] 4.1 `config.rs`: lock path `.github/workflows/actions.lock`.
- [ ] 4.2 `tidy`, `init`, `upgrade`: build workflow map and transitive graph; completeness repair for missing IDs/children.
- [ ] 4.3 Migration from `gx.lock` (reuse existing TOML readers as input only); stale-file warning when both exist.
- [ ] 4.4 `audit`: read `actions.lock`, include transitive dependencies, `mutable-ref` from `ref`.
- [ ] 4.5 `lint`: update any rule that reads the lock path.

## 5. Tests

- [ ] 5.1 Integration: fresh `gx tidy` writes `actions.lock`; composite child appears; `./` workflow skipped with warning.
- [ ] 5.2 Integration: migration from a `gx.lock` fixture makes no `lookup_sha` call and deletes the old file; both-present case warns and leaves `gx.lock` alone.
- [ ] 5.3 Optional script `mise run lock:conformance` that parses gx output with the upstream Go parser.

## 6. Repository and docs

- [ ] 6.1 Run the migration on this repository (`.github/gx.lock` → `.github/workflows/actions.lock`).
- [ ] 6.2 Update `.pre-commit-config.yaml` hook file patterns and `openspec/specs/lockfile-integrity/spec.md` to the new path.
- [ ] 6.3 Update README (Configuration, FAQ, audit section), `docs/demo.tape`, `docs/upgrade-workflow.md`, `docs/gx-upgrade.yml` for the new path; CHANGELOG breaking-change note.
- [ ] 6.4 On archive, fold the spec deltas into `openspec/specs/`.
