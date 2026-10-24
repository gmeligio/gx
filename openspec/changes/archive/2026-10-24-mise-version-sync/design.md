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

**1. CI verifies through a locked install, set explicitly with `MISE_LOCKED: 1` in each workflow's top-level env, not through a diff job.**
- Locked mode fails on a missing entry, and the download fails on a checksum mismatch. Those are the real-drift cases `lock:check` caught. Cross-version asset choice was its false positive.
- The env lives where `MISE_VERSION` sits today, so the backstop is owned by the repo, not by mise-action's default. Since v4.1 mise-action also passes `--locked` itself, which is redundant but harmless.
- Locked mode also covers `mise run` steps, so no later step can write the lock.
- Alternatives:
  - Rely on the action's automatic `--locked`. A future action release or an `install_args` override would turn the backstop off without any error.
  - Set `locked = true` in `.config/mise.toml`. That also makes local installs strict, outside this change's scope.
  - Keep `lock:check` alongside locked mode. The tree can no longer go dirty, so the job would be dead weight.
  - Use autofix.ci as the single writer from CI. That adds an app and a workflow, and produces a bot commit after every mise release.

**2. Upgrade with `gx upgrade --latest jdx/mise-action`**, so gx.toml/gx.lock and every workflow `uses:` SHA move together. This is gx's own flow, and Renovate is not configured.

**3. Float the CI mise version (v5 default: newest stable ≥24h old). No pin anywhere.**
- Alternative: an exact pin that follows the lock writer (PR #230). It needs the version copied into 5 places, a hook that edits workflows, and a hard `min_version` that rejects older contributors. Under a read-only CI, sync buys nothing.

**4. Delete `min_version`.**
- Its only job was carrying the pin.
- A floor is speculative. The lockfile has no format-revision field today, and reading works in both directions: 2026.8.5 reads a 2026.10.13 lock, and 2026.10.13 reads a 2026.8.5 lock.
- A contributor whose mise is too old to parse the config gets mise's own parse error instead of a `min_version` message. That's accepted for a project with one maintainer.

**5. Run the mise hook only when `.config/mise.toml`, `rust-toolchain.toml`, or `.config/mise.lock` changes, not `always_run`.**
- `always_run` existed to catch drift from a binary upgrade, and that drift is no longer a failure.
- Scoping it removes unrelated lock churn from PRs.
- The first two files are the lock's only tracked inputs. The lock itself is included so a hand-edited or merge-conflicted lock gets repaired. `.config/mise.local.toml` is gitignored and local-only (demo tools); its own lock is `mise.local.lock`. Task files don't affect lock content.

**6. `locked = false`: delete the line and its comment if CI's locked install of `core:rust` passes.**
- The comment claims a catch-22 under `--locked`, but `install --locked --dry-run` with rust passes on both 2026.8.5 and 2026.10.13.
- `false` is the default, so deleting the line changes no local behavior.

## Risks / Trade-offs

- **[Risk]** The `core:rust` catch-22 resurfaces under locked mode on a fresh runner.
  - **Mitigation:** the PR's own CI run is the test.
  - **Fallback:** none that keeps CI read-only. Dropping locked mode brings back CI lock rewrites. If it fails, pause implementation and revise this design.
- **[Risk]** A mise release changes the lockfile format, and a local mise up to 24h newer than CI's writes a lock CI can't parse.
  - **Mitigation:** the spec scopes acceptance to the same format. Reading across 2026.8.5–2026.10.13 is tested in both directions. On failure, set `minimum_release_age: 0s`.
- **[Risk]** A mise regression turns every PR red with no repo commit, because CI's mise floats.
  - **Mitigation:** the 24h release-age gate filters most bad releases.
  - **Fallback:** temporarily add `version: <last good>` to the affected mise-action steps and remove it once mise ships a fix.
- **[Risk]** v5.1.1 no longer exports `MISE_GITHUB_TOKEN` to later steps. A lazy tool install or GitHub API call in a later step could be rate-limited.
  - **Mitigation:** with `MISE_LOCKED: 1` and every tool installed by the action, later `mise run` steps don't install lazily.
  - The steps that do call the GitHub API already set their own token: `mise run e2e` (`GITHUB_TOKEN`), release-plz (app token), and `release.yml` (`GH_TOKEN`).
- **[Risk]** `release.yml` macOS/Windows build jobs run only on tags, so a PR won't exercise their locked install.
  - **Mitigation:** the lock already has `macos-*` and `windows-x64` entries for every tool that has platform entries. Watch the first release.
- **[Trade-off]** The lockfile reflects the last writer's mise asset choices, for example gnu vs musl. Both are checksum-pinned, and CI honors whichever is locked.
- **[Trade-off]** A locked install does not flag an orphan lock entry left behind after a tool is removed from `mise.toml`; the old dirty-tree diff did. This is accepted: an orphan entry installs nothing and is pruned the next time the hook runs `mise lock` (verified on 2026.10.13), which happens on that same `mise.toml` edit.

## Migration Plan

Land in a single PR. Rollback is a revert. Contributors need no action; any mise release that parses the config works (see Decision 4).

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
- CI can no longer rewrite the lock, so release-plz's dirty-tree abort from tool installs disappears.
- Remaining silent path: someone deletes `MISE_LOCKED` from a workflow. That needs a deliberate edit, which review catches; it can't drift in through an action upgrade.
- The trade-off: there is no single `Lockfile` job name; the failure appears in every job.
