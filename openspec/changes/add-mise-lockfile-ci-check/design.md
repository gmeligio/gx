## Context

`.config/mise.toml` sets `lockfile = true`, so every `mise install` writes `.config/mise.lock`. `mise-action` runs `mise install` unconditionally (its `install` input defaults to `"true"`; the cache only preseeds `~/.local/share/mise`), and its `version` input is unspecified in all 16 call sites — the action's docs say "if not specified, will use the latest release". So all 8 `build.yml` jobs are lockfile writers driven by a binary whose version CI does not pin, and no job checks the result.

The write that dirtied the tree on 2026-08-03 correlates with the mise **cache miss**, not with the commit: the failing run installed all 5 tools fresh (10 install lines), while the two runs that passed minutes later restored the cache and reinstalled only `rust` (2 lines), whose `core:rust` backend writes no lock entry. The four archive-backed tools — prek, shellcheck, cargo-deny, cargo-dist — are the ones that touch the lockfile, and only on a cold install.

The exact field is unidentified and the mise version's role is unconfirmed. mise 2026.8.1 has since run three times on `main` leaving the lockfile clean, so a newer CI binary is not sufficient on its own; a cold install under 2026.7.13 with an isolated data dir also reproduced clean. The trigger appears to need a cold install *and* something version-specific together, untestable in the investigation environment (mise binary downloads truncate at 64K there). This design deliberately does not depend on that answer — the check compares mise's output to the committed file, so it detects the drift whatever produces it, and prints the field.

`locked = false` in `.config/mise.toml` is load-bearing here and constrains the solution: mise ≥ 2026.6.0 cannot write a lock entry for the `core:rust` backend resolved from `rust-toolchain.toml`, yet under `locked = true` it *requires* rust to be in the lockfile. CI therefore cannot validate the lockfile with `--locked`; it can only let mise rewrite it and compare.

Verified behaviors underpinning this design (all exercised on a throwaway clone):

- A **warm** `mise install` against a satisfied tree leaves `git diff --exit-code -- .config/mise.lock` at 0 — the check is not inherently flaky.
- `mise install` **repairs** a drifted lockfile in place: hand-injected drift was reverted to mise's fixed point.
- `git diff --exit-code -- .config/mise.lock` returns 1 on drift, 0 when clean.
- With `&& git add -u`, the prek hook reverts *and* stages, then a re-run passes — the same two-step experience `cargo-fmt` already produces (confirmed by triggering a real formatting violation: `Failed` + "files were modified by this hook", fix staged, retry clean).
- The failing and passing release-plz runs differed only in mise cache state (miss → all 5 tools installed fresh → dirty; hit → rust only → clean), which is why the drift is intermittent rather than deterministic.

## Goals / Non-Goals

**Goals:**
- Drift in `.config/mise.lock` fails the PR that surfaces it, not the next release.
- The failure names the offending field (the diff is printed), rather than release-plz's misleading "committed and in .gitignore" message.
- Locally, drift is auto-repaired and re-staged instead of blocking, matching `cargo-fmt` / `cargo-linter` / `gx-lockfile`.
- `mise install` becomes a mise task, bringing the last inline tool invocation under `task-execution-consistency`.

**Non-Goals:**
- Pinning mise's version. That removes the *cause*; this change detects the *effect*. Worth doing separately.
- A `tidy:check` for `.github/gx.lock`. Its writer is gx-compiled-from-source, pinned by this repo, so it has no floating input and cannot drift this way. Adding a guard for a bug that cannot currently occur is speculative.
- Making CI self-heal by committing the regenerated lockfile. CI must not mutate the tree (`task-execution-consistency`: "CI SHALL NOT modify the working tree").

## Decisions

**Decision 1: name the pair after the writer — `lock` / `lock:check`.**
`task-execution-consistency` requires that fixing checks "mutate in the local and pre-commit environments and SHALL be verified non-mutating in CI", with "only the verb differs". `lock` (mutating, `mise install`) and `lock:check` (verifying) instantiate that pattern exactly, matching `format`/`format:check` and `clippy`/`clippy:check`.

Rejected: `lint:lockfile`. The `lint:` namespace holds source-level budgets — its sole member `lint:size` runs `cargo test --test code_health`, whose 9 tests are pure Rust-source introspection (`domain_does_not_import_upward`, `import_path_hygiene`, `mod_rs_reexports_only`, …) and never read `.config/` or `.github/`. A lockfile verifier there would be a category error, and would name the artifact rather than the writer, breaking the pattern for the eventual `tidy:check`.

Rejected: adjusting the existing naming pattern for simplicity. It is one hop from `mise run <task>` to the command with `:check` as the only variation — already minimal — and it is load-bearing: PR #102's `cargo-deny` failure happened precisely because a flag lived in one spelling and not another.

`lock/` has no naming hazard: it is a new directory, and `lock/_default` yields the bare `lock` name (the file-based convention established in #103).

**Decision 2: `lock:check` re-runs `mise install` rather than assuming mise-action already did.**
The task is then self-contained and runnable locally without depending on a prior step, satisfying "every environment invokes it identically". The redundant install in CI is a warm no-op (verified), costing seconds.

**Decision 3: scope the diff to `.config/mise.lock`, and let it print.**
`git diff --exit-code -- .config/mise.lock` rather than a bare `git status` check: a bare check would trip on incidental writes from other tooling and reproduce exactly the misleading signal release-plz gives today. `--exit-code` prints the patch by default, naming the field mise started writing — the thing that required separate investigation at #66 and #112.

**Decision 4: the local hook auto-fixes and re-stages; CI verifies.**
`mise install` already repairs the file, so the hook only lacked `git add -u`. Adding it makes the hook consistent with its three siblings. This contradicts the current `lockfile-integrity` requirement that a hook-modified lockfile blocks the commit, so that requirement is amended for `.config/mise.lock` rather than silently violated. Note the honest limit: prek still reports `Failed` on the run that fixes things — "auto-fix" means one-shot repair with the fix staged, not an invisible commit. That is exactly how `cargo-fmt` behaves today.

**Decision 5: reverse Decision 5 of `2026-06-06-add-prek-lockfile-hooks`.**
That decision deliberately omitted this CI job ("it was the reverted 'Lockfiles' job … deliberately not re-added") on the reasoning that the only route to CI drift is a bypassed or un-installed local hook, implausible for a solo, always-bootstrapped repo. The premise held; the gap bit anyway through a route it did not consider — **drift originating in CI's own install conditions**, not in a contributor's commit. The hooks were installed and the committed lockfile was correct; what varied was whether CI's mise cache hit. No hook installation could have prevented that, because nothing about the commit was wrong. The decision is reversed on new evidence, not overruled.

**Decision 6: `lock:check` joins the `test` gate's `depends`.**
`test/_default` documents "Keep `depends` in sync with the PR-check jobs in build.yml", and `task-execution-consistency` requires the gate to "reproduce the verdict of the CI PR-check jobs". Adding the CI job without the gate member would break both. Locally the member is normally a fast pass, because the prek hook has already reconciled the lockfile before the commit lands.

## Risks / Trade-offs

- **A cache miss turns an unrelated PR red** → The gate fires on whichever PR first runs with a cold mise cache after the lockfile stops being a fixed point; that author neither caused nor can cleanly fix it. Mitigated by the local auto-fix making the remedy mechanical (`mise run lock`, commit) and by the printed diff naming the field. Note this is the failure surfacing, not the gate creating it — today the same drift reaches the release pipeline instead, where it is both later and harder to read.
- **The redundant `mise install` in `lock:check`** → Warm no-op, seconds; buys a self-contained task. Accepted.
- **Ninth parallel job** → No change to serial critical path; ~30s wall-clock, in parallel with 8 existing jobs.
- **The auto-restage weakens the "contributor sees the regenerated lock" signal** → prek still reports `Failed` and names the hook, so the regeneration remains visible; only the manual `git add` disappears.
- **The gate is only as cold as CI's cache** → `lock:check` runs after `mise-action`, so on a cache hit it verifies a warm install and cannot surface cold-install-only drift. It would therefore have passed on the two runs that followed the failure. This is a real coverage limit, accepted: the gate still catches the drift on the same cache-miss runs that trigger it, which is where the release pipeline catches it today — only sooner and with a readable diff.
- **The gate cannot distinguish "mise changed its output" from "someone hand-edited the lockfile"** → Both are drift and both warrant the same fix (regenerate). No mitigation needed; the printed diff disambiguates for a human.

## Migration Plan

1. Confirm `.config/mise.lock` is a fixed point for the current mise (`mise install` produces no diff) — it is, and `main` is green, so no regeneration is needed before the gate exists.
2. Add the `lock` / `lock:check` tasks, then wire the hook, the `test` gate, and the CI job.
3. Rollback is deleting the `Lockfile` job and the `depends` entry; the tasks and hook change are independently harmless.

No sequencing constraint against other work — the change touches no application code.

## Automated Test Strategy

Verification is behavioral, exercised locally against a throwaway clone (no new test framework; prek, mise, and git are the harness). The critical path is the drift-detection loop, and each leg has already been rehearsed during investigation:

- **`lock:check` fails on drift**: inject a line into `.config/mise.lock`, run `mise run lock:check`, expect non-zero exit and the offending line in the printed diff.
- **`lock:check` passes when clean**: run it twice against an untouched tree, expect exit 0 both times (guards against the task itself being a writer that dirties the file).
- **Hook auto-fixes and re-stages**: stage an unrelated file, inject drift, commit; expect the hook to report `Failed` with "files were modified by this hook", the lockfile reverted and staged, and an immediate re-commit to succeed.
- **Gate membership**: `mise run test` resolves `lock:check` (confirm it appears in the run), and a drifted lockfile fails the gate.
- **CI job**: confirm the `Lockfile` job appears and passes on this change's own PR — which also proves step 1 of the migration plan worked.

The one case that cannot be reproduced on demand is the real trigger — a cold-cache CI install that rewrites the lockfile. Neither a cold install nor a newer mise reproduced it in isolation, and the combination was untestable locally. It is simulated by hand-injected drift, which exercises the same detection path (`mise install` writes, `git diff` compares) even though it originates the drift differently.

## Observability

- **CI**: the `Lockfile` job fails with `git diff --exit-code`'s own patch output, naming the exact field and tool that drifted. This is the primary improvement over the status quo, where the same drift surfaced as release-plz's `the working directory of this project has uncommitted changes` plus a misleading hint about `.gitignore` — a message that sent the original investigation down the wrong path (the file is neither gitignored nor untracked; `git ls-files -ci --exclude-standard` is empty).
- **Per-check attribution**: `task-execution-consistency` requires separate jobs so a failure names the check. `Lockfile` is its own job, so the red X reads as a lockfile problem rather than being buried in `Check`.
- **Local**: prek prints the hook name and "files were modified by this hook" when it regenerates, so the repair is visible rather than silent.
- **No silent-failure path**: the check is a single non-zero exit; there is no log-and-continue branch. The one way it can pass while drift exists is if `mise install` fails outright — in which case the task fails on that instead, still non-zero.
