## Why

`docs/renovate.md` already documents the two-layer model and the in-range limitation, but three of its
load-bearing external claims are unsourced or wrong, and one is now demonstrably out of date. A page whose whole
purpose is to be *honest* about a boundary loses its authority when its evidence does not hold up:

- The strongest available proof — Renovate's own `rangeStrategy` docs enumerate a **closed list of managers**
  that support `update-lockfile`, and `regex`/custom is not on it — is not cited at all. The page instead
  argues from a 2022 issue thread (`renovate#19802`), which is weaker and ages worse.
- The page names npm, uv, and pnpm as *analogues* (the two-layer table, lines 11–14) but never says what
  those ecosystems do that gx cannot: each has a **native Renovate manager that maintains the lock**. Without
  that column the reader sees a shared shape and no explanation of why only gx has the gap — which is exactly
  the design rationale issue #109's acceptance criterion asks the docs to make legible.
- One factual claim is wrong. Line 146 says a future native gx manager would detect "a gx project by
  `gx.lock` … exactly as Renovate's native uv and pnpm managers do." Renovate's uv support does not work that
  way: the PEP 621 manager matches `pyproject.toml` and lists `uv.lock` in `lockFileNames`. The analogy
  misdescribes the tool it appeals to.

## What Changes

- In the "what Renovate cannot do" section, lead with the primary source: the `rangeStrategy` option docs,
  which name the managers `update-lockfile` works for and exclude custom managers. Keep `renovate#19802` only
  where it still earns its place (line 142, as evidence that Renovate's schema drifts).
- Say what npm, uv, and pnpm do that gx cannot: each has a **native Renovate manager that maintains the lock**
  (delegating the regeneration to the ecosystem's own CLI), while gx is attached by a custom regex manager
  blind to `gx.lock`. Add this as short prose next to the existing analogue table — not as a fourth column,
  which would be meaningless for the `gx.toml` row, and not as a second table duplicating lines 11–14.
- Fix the uv analogy on line 146 so it describes how Renovate's PEP 621 manager actually works
  (`pyproject.toml` file match, `uv.lock` in `lockFileNames`) rather than detection by lock presence.

No change to `src/`, `tests/`, or CLI behavior. The gx#121 reference on line 131 stays as-is: that workflow
template has not shipped to this tree, so "tracked in gx#121" remains accurate.

Constraint for the edit: the statement "Renovate catches majors; in-range advancement is `gx upgrade`'s job"
(line 54) is the page's thesis and must stay prominent.

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
