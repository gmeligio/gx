## Why

GitHub now ships a first-party lockfile for Actions dependencies: `.github/workflows/actions.lock`, in public preview since 2026-10-14 ([changelog](https://github.blog/changelog/2026-10-14-github-actions-workflow-lockfiles-in-public-preview)). The format is defined by [`github/actions-lockfile`](https://github.com/github/actions-lockfile) (schema [`v0.0.3`](https://github.com/github/actions-lockfile/blob/main/schema/lockfile-v0.0.3.json)) and produced today by [`gh actions-lock`](https://github.com/github/gh-actions-lock). Unlike `.github/gx.lock`, the runner itself reads it: for every workflow listed in it, the runner checks at job start that each `uses:` is locked, that the locked commit is what runs, that the repository behind the name has not been transferred or re-registered (numeric owner/repo IDs), and that the commit is reachable from the recorded ref ([how it works](https://github.com/github/gh-actions-lock#how-it-works)).

`gx.lock` records the same facts gx needs (which commit each version resolved to) but nothing outside gx reads it. A user who wants runner enforcement has to run a second tool that maintains a second lockfile describing the same pins. Writing GitHub's lockfile instead makes gx's lock enforceable by the platform, readable by every tool that consumes the official format, and removes a gx-only file from users' repositories.

Gate: this requires a spec. It changes the lock file's path, format and content, which users see, commit and review.

## What Changes

- **BREAKING**: gx reads and writes `.github/workflows/actions.lock` in the GitHub schema (`version: 'v0.0.3'`) instead of `.github/gx.lock`. The output is byte-compatible with what `gh actions-lock` writes for the same pins, so the two tools can run on the same repository without churning the file.
- gx records what the schema requires and gx does not track today:
  - numeric `owner_id` and `repo_id` for every locked repository;
  - a `workflows:` map from each workflow path to the flat, transitive list of pins it depends on;
  - transitive dependencies: the `uses:` steps inside remote composite actions, resolved and locked as their own entries and linked through `uses:`.
- `gx tidy` keeps rewriting workflows to commit SHAs with a version comment. In the lockfile that pin's key is `owner/repo@<sha>` and its `ref` is the version gx resolved (the same shape `gh actions-lock` writes for a SHA-pinned `uses:`). Runner enforcement is an addition to SHA pinning, not a replacement: the preview is limited to GitHub Enterprise Cloud, and repositories without it keep the immutability they have today.
- Workflows that GitHub cannot onboard (any step using a `./` local action) are left out of `workflows:` rather than written in, so gx never makes the runner reject a workflow that runs today. gx warns once per such workflow.
- The resolution tier (`[resolutions]` specifier → version) and the `date` field are dropped. Both are derivable: the locked `ref` of a pin already names the version that satisfies the manifest specifier, and nothing reads `date`.
- `gx.lock` is migrated on the first `gx tidy`/`gx init`/`gx upgrade` that finds it and no `actions.lock`: its pins are carried over (no re-resolution of versions), IDs are fetched, `actions.lock` is written and `gx.lock` is deleted. The migration is reported.
- `gx audit` reads `actions.lock`. Its `mutable-ref` check classifies refs from the locked `ref` value.
- `.github/gx.toml` (the manifest of user intent) is unchanged.

## Capabilities

### New Capabilities

None. The lock is an existing concept; this changes where and how it is stored and what it records.

### Modified Capabilities

- `manifest-and-lock`: lock path, format, structure, completeness, orphan cleanup, migration.
- `action-resolution`: resolving a pin now also yields repository IDs and the pin's own transitive dependencies.
- `audit-command`: reads `actions.lock`; `mutable-ref` classifies from the locked `ref`.
- `lockfile-integrity`: the tracked lockfile for this repository becomes `.github/workflows/actions.lock` (task only; requirement text names the file).

## Impact

- **Replaced**: `src/infra/lock/` (TOML two-tier reader/writer) by a YAML reader/writer for the v0.0.3 schema. The legacy TOML readers stay as migration-only input.
- **Modified**: `src/domain/lock.rs` (entries keyed by pin, per-workflow membership, `uses` edges), `src/config.rs` (lock path), `src/tidy/` (lock sync builds the workflow map and transitive graph), `src/init/`, `src/upgrade/`, `src/audit/`.
- **New network calls**: `GET /repos/{owner}/{repo}` (owner and repo IDs, once per repository, cached) and a read of `action.yml`/`action.yaml` at the locked commit for each remote action (to discover composite dependencies). Both go through the existing `Caching<Retrying<…>>` registry stack.
- **Dependencies**: none new. `serde-saphyr` already parses YAML; the writer emits the fixed shape directly (see design).
- **Users**: one-time diff that deletes `.github/gx.lock` and adds `.github/workflows/actions.lock`. README, `docs/demo.tape`, and `.pre-commit-config.yaml` hook file patterns are updated.
