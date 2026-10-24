## Context

See proposal.md (Why) and research.md (evidence).

- CI uses `jdx/mise-action` v3.6.1 because `.github/gx.toml` pins `^3`. That version runs a plain `mise install`, and with `locked = false` the install rewrites lock entries.
- Every lock entry already carries URL + checksum for 7 platforms, including the macOS and Windows runners of `release.yml`.

Tested on 2026-10-24, isolated data dir, using main's lock as written by 2026.10.13:

- mise 2026.8.5, plain install: rewrote prek's `linux-x64` entry from gnu to musl.
- mise 2026.8.5, `install --locked`: left the lockfile unchanged and installed the locked gnu asset.

## Goals / Non-Goals

**Goals:**
- One lockfile writer: the contributor's local mise, whatever version it is.

**Non-Goals:**
- A byte-identical lockfile across mise versions.
- Making a contributor's local mise auto-update.
- Adopting Renovate. Its mise manager would add a third writer.

## Decisions

**1. CI verifies through mise-action's automatic `--locked` (v4.1+), not through a diff job.**
- `--locked` fails on a missing entry, and the download fails on a checksum mismatch, which is everything `lock:check` guarded against except cross-version asset choice. Cross-version asset choice was the false positive.
- Alternatives:
  - Keep `lock:check` alongside `--locked`. The tree can no longer go dirty, so the job would be dead weight.
  - Use autofix.ci as the single writer from CI. That adds an app and a workflow, and produces a bot commit after every mise release.

**2. Upgrade with `gx upgrade --latest jdx/mise-action`**, so gx.toml/gx.lock and every workflow `uses:` SHA move together. This is gx's own flow, and Renovate is not configured.

**3. Float the CI mise version (v5 default: newest stable ≥24h old). No pin anywhere.**
- Alternative: an exact pin that follows the lock writer (PR #230). It needs the version copied into 5 places, a hook that edits workflows, and a hard `min_version` that rejects older contributors. Under a read-only CI, sync buys nothing.

**4. Delete `min_version`.**
- Its only job was carrying the pin.
- A floor is speculative. The lockfile has no format-revision field today, and the tested old reader accepts the new lock.

**5. Run the mise hook only when `.config/mise.toml` or `rust-toolchain.toml` changes, not `always_run`.**
- `always_run` existed to catch drift from a binary upgrade, and that drift is no longer a failure.
- Scoping it removes unrelated lock churn from PRs.

**6. `locked = false`: delete the line and its comment if CI's locked install of `core:rust` passes.**
- The comment claims a catch-22 under `--locked`, but `install --locked --dry-run` with rust passes on both 2026.8.5 and 2026.10.13.
- `false` is the default, so deleting the line changes no local behavior.

## Risks / Trade-offs

- **[Risk]** `core:rust` catch-22 resurfaces under the automatic `--locked` on a fresh runner.
  - **Mitigation:** the PR's own CI run is the test.
  - **Fallback:** set `install_args: ""` on mise-action to opt out. The action respects explicit `install_args`.
- **[Risk]** Local mise is newer than CI's latest−24h and writes a lock CI can't read.
  - **Mitigation:** tested backward reading. The window is ≤24h. On failure, set `minimum_release_age: 0s`.
- **[Risk]** v5.1.1 no longer exports `MISE_GITHUB_TOKEN` to later steps. A lazy tool install or GitHub API call in `mise run …` steps could be rate-limited.
  - **Mitigation:** e2e already passes `GITHUB_TOKEN` explicitly. Any other job that hits limits gets `persist_github_token: true`.
- **[Risk]** `release.yml` macOS/Windows build jobs run only on tags, so a PR won't exercise their locked install.
  - **Mitigation:** the lock already has `macos-*` and `windows-x64` entries for every tool that has platform entries. Watch the first release.
- **[Trade-off]** The lockfile reflects the last writer's mise asset choices, for example gnu vs musl. Both are checksum-pinned, and CI honors whichever is locked.

## Migration Plan

Land in a single PR. Rollback is a revert. No contributor action is needed beyond having a mise version recent enough to parse the config.

## Automated Test Strategy

No Rust code changes. The critical path is the PR's own CI:

- every job's locked install must pass with the floating mise;
- `git status` must stay clean after install.

Locally:
- `prek run --all-files`: hooks still pass;
- `mise run test`: the gate passes without `lock:check`.

## Observability

- A stale lock fails the first tool-installing step of each job, naming the tool with no matching entry.
- A checksum mismatch fails the download.
- No silent path: CI can no longer rewrite the lock, so release-plz's dirty-tree abort from tool installs disappears.
- The trade-off: there is no single `Lockfile` job name; the failure appears in every job.
