## Why

Issue #133. When `gx audit` (issue #129) reports a vulnerable action, the obvious
next question is "how do I fix it?" — and gx already owns the fix: `gx upgrade`.
But `gx upgrade` only ever moves an action *within its manifest specifier*. So
the honest suggestion is not always available:

| Manifest specifier | Current | Fixed in | Reachable by `gx upgrade`? |
|---|---|---|---|
| `^46` (tj-actions) | 46.0.0 | 46.0.1 | yes — same major |
| `~2.37` (setup-php) | 2.36.0 | 2.37.1 | yes — same minor line |
| `^2` (codeql-action) | 2.26.11 | 3.0.0 | **no** — fix is a major bump |
| `^0.34` (trivy-action) | 0.30.0 | 0.35.0 | **no** — 0.x caret is patch-locked |

And 5 of the 63 real `ACTIONS` advisories carry no `firstPatchedVersion` at all
(`njzjz/wenxian <= 0.3.1`, `reviewdog/action-setup = 1`,
`github/codeql-action >= 2.26.11, < 3.0.0`, …) — there is no version to reach.

A remediation suggestion that does not work is worse than none: the user runs it
during a security incident, nothing changes, and they lose trust in the tool at
the worst possible moment. So the decision must be **provable, not optimistic** —
suggest the command only when the fix is machine-checkably in range.

This change lands only that decision. The audit command, the finding type, and
the rendering belong to #129/#130 and are explicitly out of scope.

## What Changes

- Add a domain concept `Remediation` in `src/domain/remediation.rs`: given a
  manifest `Specifier` and an advisory's `firstPatchedVersion` (possibly absent),
  it classifies the fix into exactly one of three outcomes:
  - `Upgradable { fixed }` — a patched version exists and the specifier admits
    it; `gx upgrade <action>` will reach it.
  - `NoFixAvailable` — the advisory names no patched version, or names one gx
    cannot deliver (unparseable, or a prerelease no range admits); migration
    required.
  - `OutOfRange { fixed }` — a patched version exists but the specifier cannot
    reach it; the manifest entry must change — a wider range, typically across a
    major, or for a branch/SHA pin a semver range at all.
- Reuse `Specifier::matches_version` for condition 2 rather than adding new
  matching logic, and widen the crate's existing `parse_semver` from
  `pub(super)` to `pub(crate)` rather than duplicating it. Normalize advisory
  identifiers through `Version::normalized`
  so the `v`-prefix mismatch between advisory strings (`3.0.0`) and gx tags
  (`v3.0.0`) cannot produce a wrong verdict.
- No CLI surface, no output, no network. Nothing calls this yet.

## Capabilities

### New Capabilities
- `remediation-guidance`: whether gx may tell a user that `gx upgrade <action>`
  fixes a known vulnerability, and what it says when it may not.

### Modified Capabilities

_None._ `upgrade-operations` describes how `gx upgrade` behaves; this change
does not alter that behavior, it only decides when to point at it.

## Spec relevance gate

**Requires a spec.** Deliberately, despite no user-visible behavior shipping in
this change — the gate is about the *nature of what is being decided*, not
whether a caller exists yet.

- Gate "introduces a new domain concept that changes what users can do": met.
  The three-way classification is not an implementation detail of some other
  rule — it *is* the policy that determines which of three different things a
  user is told during a security incident, and whether they are handed a command
  that works. Later issues render this decision; they do not author it. If it
  went unspecified, the constraint that gx must never suggest an unreachable
  upgrade would live only in a function body, and #129/#130 would be free to
  reinvent it more optimistically.
- "Skip spec: internal refactoring with no user-visible change": does **not**
  apply. Nothing is being restructured; a behavioral rule is being created.
- "Skip spec: would duplicate an existing spec": does **not** apply.
  `upgrade-operations` covers executing an upgrade, never advising one; no
  existing capability covers vulnerability remediation.

The spec is written in user-observable terms (what the user is told, what
command they are or are not given) so it stays the contract that #129/#130 must
satisfy when they wire the rendering. What is deferred is only *where the text
appears*, which is those issues' business.

## Impact

- New file: `src/domain/remediation.rs` (+ one `pub mod` line in
  `src/domain/mod.rs`). `src/domain/action/` is at its 8-file budget, and this
  concept is about advisories, not about the action identity model, so it sits
  one level up in `src/domain/`.
- Reads existing `Specifier`, `ResolvedRef`, and `Version` — changes none of
  them. The one edit outside the new file is widening `parse_semver`'s
  visibility in `src/domain/action/specifier.rs`; it stays crate-private.
- No new dependency (`semver = "1"` is already used via `Specifier`).
- No behavior change for any existing command; `gx` output is byte-identical.
