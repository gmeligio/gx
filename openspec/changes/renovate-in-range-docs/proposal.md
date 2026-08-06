## Why

`docs/renovate.md` already documents the two-layer model and the in-range limitation, but three of its
load-bearing external claims are unsourced or wrong, and one is now demonstrably out of date. A page whose whole
purpose is to be *honest* about a boundary loses its authority when its evidence does not hold up:

- The strongest available proof — Renovate's own `rangeStrategy` docs enumerate a **closed list of managers**
  that support `update-lockfile`, and `regex`/custom is not on it — is not cited at all. The page instead
  argues from a 2022 issue thread.
- The npm/uv/pnpm comparison that makes the design rationale legible (per issue #109's acceptance criterion)
  is missing entirely from the page; only uv is mentioned, in passing.
- The page points users at gx#121 as a *future* copy-paste workflow. That framing is now stale.

## What Changes

- Replace the `renovate#19802` inference in the "what Renovate cannot do" section with the primary source: the
  `rangeStrategy` option docs, quoting the closed manager list that excludes custom managers, plus the custom
  regex manager's capture-group list (no `lockedVersion`).
- Add the npm/uv/pnpm comparison table the issue's acceptance criterion calls for, so the reader sees gx is not
  uniquely broken — it is uniquely *attached by a custom regex manager*, and the fix shape is a native manager.
- Correct the uv detection claim inherited from issue #109: Renovate does **not** detect uv by the presence of
  `uv.lock`. The PEP 621 manager matches `pyproject.toml` and names `uv.lock` in `lockFileNames`.
- Reword the scheduled-`gx upgrade` section so it stands on `gx upgrade --json` (which exists today) rather
  than promising a workflow template file that is not in the tree.

No change to `src/`, `tests/`, or CLI behavior.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None.

**Spec decision — skip, justified against the relevance gate.** The gate requires a spec when a change "adds,
removes, or changes user-facing behavior" or "introduces a new domain concept that changes what users can do."
Neither applies. Every behavior this page describes already ships and is already specified: the manifest/lock
split is `manifest-and-lock`, and in-range lock advancement is `upgrade-operations`. This change alters only the
prose describing that behavior and the citations backing it — a user's observable interaction with gx is
byte-identical before and after. Writing a spec here would "duplicate an existing spec," which the gate lists as
an explicit skip condition.

## Impact

- `docs/renovate.md` — the only file changed.
- `README.md` — no change needed; line 75 already carries the one-line pointer with the exact framing issue #109
  asks for ("It catches majors; in-range advancement is `gx upgrade`'s job").
- `docs/demo.tape` — no change. It records a terminal session of CLI commands; per `AGENTS.md` it is updated when
  "changes affect user-facing behavior, commands, config, or installation." This change adds no command and
  alters no output.
- Out of scope: the `gx lint` rule half of issue #109, which is queued separately.
