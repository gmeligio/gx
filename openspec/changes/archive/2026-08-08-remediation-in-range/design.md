## Context

Issue #133 is the remediation half of the `gx audit` series (#129 command shell,
#130 vulnerability check). Neither has landed: **`gx audit` does not exist yet**.
This change therefore ships only the decision logic — a pure function from
(manifest specifier, advisory first-patched-version) to one of three outcomes —
so that #130/#129 can call it rather than reinvent it, more optimistically, at
render time.

Existing material this builds on:

- `Specifier` (`src/domain/action/specifier.rs`) already models `Range` / `Ref` /
  `Sha` and already parses `^`/`~` into a `semver::VersionReq`.
- `Specifier::matches_version(&ResolvedRef)` already implements range matching
  including the `v`-prefix-tolerant `parse_semver`.
- `Version::normalized` already canonicalizes a bare `46.0.1` to `v46.0.1`.
- `semver = "1"` is already a dependency.

## Goals / Non-Goals

**Goals:**

- Classify a fix into exactly `Upgradable` / `NoFixAvailable` / `OutOfRange`.
- Be conservative: any doubt resolves to "no command".
- Reuse existing matching rather than adding a second, drifting notion of range.
- Unit-test all three branches against the four real advisory cases in #133.

**Non-Goals:**

- No `src/audit/`, no `gx audit` command, no finding type, no output rendering,
  no severity, no GHSA plumbing, no network. Those are #129/#130.
- No formatting of the user-facing sentences ("migration required", "requires a
  major bump"). The spec fixes *what* must be conveyed; #130 picks the wording
  and owns the string. This change exposes the distinction, not the prose.
- No suggestion of *which* major to move to, and no `gx upgrade --major` style
  escape hatch.

## Decisions

### Name and placement: `Remediation` in `src/domain/remediation.rs`

The concept is "what can be done about a known-vulnerable action", which is
about advisories, not about the action identity model. `src/domain/action/` is
also at the hard 8-file budget, so a file there is not available regardless.
`src/domain/` has a free slot; the file lives there as `remediation.rs`.

Alternative considered: appending to `specifier.rs`. Rejected — it would grow a
file already near its total-line budget, and more importantly it would bury a
security-policy decision inside the generic specifier type, which is exactly the
"lives only in a function body" failure the proposal argues against.

### Shape: an enum, not a `bool` or an `Option<String>`

```rust
pub enum Remediation {
    Upgradable { fixed: Version },
    NoFixAvailable,
    OutOfRange { fixed: Version },
}
```

Returning a `bool` ("suggest or not") would collapse the two no-command cases,
and the spec requires the user be told *which* obstacle applies. Returning
`Option<String>` (the command) would force this module to own the command text,
which belongs to the caller. An enum makes the three-way distinction total and
lets the compiler force #130 to handle each arm.

`fixed` is carried on both non-`NoFixAvailable` arms because the caller needs it
either way: to name the target on success, and on failure to name the version the
user must reach for — "fixed in v3.0.0, outside your ^2 range", or "fixed in
v46.0.1, which a `main` pin will never reach".

### Condition 2 reuses `Specifier::matches_version` — but only its `Range` arm

`matches_version` is sufficient for the range test, and no new matching logic is
needed. **But its non-range arms answer a different question and must not be
reused verbatim.** `matches_version` asks "is this existing pin *permitted*?" —
so a `Ref`/`Sha` specifier, and a `ResolvedRef::Commit`, are *exempt* and return
`true` (see `src/tidy/lock_sync.rs:112`, its only caller, which is drift
detection). Remediation asks "would `gx upgrade` *reach* this version?" — and for
a branch or bare-SHA specifier the answer is **no**: there is no range for
`gx upgrade` to search.

So the polarity inverts on the non-range arms. The implementation therefore
matches on the specifier: a `Range` delegates to
`matches_version(&ResolvedRef::Tag(fixed))`, while `Ref` and `Sha` classify as
`OutOfRange` directly. Wrapping the candidate in `ResolvedRef::Tag` is exact —
the patched version from an advisory is always a version tag, never a branch and
never a bare commit — so the `Branch`/`Commit` arms of `matches_version` are
unreachable from here by construction.

This gives `OutOfRange` two obstacles, not one: a range that excludes the fix,
and a pin that has no range at all. Its contract is therefore "the manifest entry
must change", *not* "a major bump is required" — the narrower sentence would be
false for a branch or SHA pin, on exactly the reasoning that keeps a prerelease
out of this arm below. The caller still holds the `Specifier`, so #130 can say
which of the two applies without a fourth variant.

Alternative considered: widening `matches_version` with a flag or a second
method on `Specifier`. Rejected — it would push a security concern into a type
used by drift detection, and risk changing `tidy`'s behavior. Keeping the
inversion local to `Remediation` is smaller and cannot regress an existing
caller.

### A prerelease fix is `NoFixAvailable`, not `OutOfRange`

Semver excludes a prerelease from any range whose bound does not itself carry
one, so `^2` does not admit `2.1.0-beta.1`. Delegating to `matches_version`
would therefore land it in `OutOfRange` — correct-by-delegation on the range
question, but the rendered sentence would be false: "outside your `^2` range —
requires a major bump" when a major bump would not reach it either. The
obstacle is the prerelease, not the specifier's width.

Classified as `NoFixAvailable` on exactly the reasoning already applied to
unparseable identifiers: gx has no fix it can deliver, so migration is the
honest advice. Implemented as an explicit `v.pre.is_empty()` filter rather than
left to `matches_version`, because the correct answer here differs from the
range verdict.

### The `v` prefix is normalized once, at the boundary

Advisory `firstPatchedVersion.identifier` strings are usually bare (`46.0.1`);
gx `Version` values usually carry a `v` (`v46.0.1`). Two defenses, both already
existing:

- On the way in, the identifier is canonicalized from the **parsed** semver —
  `Version::normalized(&parsed.to_string())`, not from the raw string. This is
  load-bearing in two ways `Version::normalized` alone does not cover:
  `parse_semver` accepts an uppercase `V`, which `normalized` would leave as
  `V46.0.1` because it only prefixes digit-leading strings; and `parse_semver`
  pads `2.37` to `2.37.0`, which the raw string would not reflect, leaving
  `fixed` range-shaped rather than a concrete version.
- On the way through, `matches_version` → `parse_semver` strips a leading `v`
  anyway, so matching is prefix-insensitive independent of the above.

Tested in both directions: `46.0.1` and `v46.0.1` must reach the same verdict.

### Unparseable patched version ⇒ `NoFixAvailable`, never `Upgradable`

An advisory identifier that does not parse as semver is treated exactly like an
absent one: `NoFixAvailable`.

The alternative — letting it fall through to `OutOfRange`, since
`matches_version` would return `false` anyway — is rejected. It is conservative
in the right direction (no command is emitted either way), but it makes gx say
something false: `OutOfRange` means "the fix exists, your specifier just won't
reach it", and #130 renders that as "fixed in X, outside your ^2 range —
requires a major bump". For an unintelligible identifier there is no coherent X
and a major bump would not help. Telling a user under incident pressure to widen
a specifier toward a version that does not exist is the same trust failure the
proposal is about, just one step removed.

Collapsing it into `NoFixAvailable` keeps both messages true: gx has no usable
patched version, so migration is the honest advice. It also keeps the enum
three-way — a fourth `Unintelligible` variant would push a data-quality detail
of the advisory feed into every caller's match, for no different user action.

Concretely: parse the identifier with `parse_semver` before classifying, and
treat an unparseable identifier the same as an absent one. This means
`Remediation` never carries a `fixed` value it could not parse. Because the
carried value is then rebuilt from the parsed semver rather than the raw string,
`Version::normalized`'s digit-leading guard cannot bite.

`parse_semver` is currently `pub(super)` inside `src/domain/action/`, so it must
widen to `pub(crate)` for `src/domain/remediation.rs` to call it. That one-word
change is preferred over reimplementing the `v`-stripping and
`4` → `4.0.0` padding locally: a second parser would be exactly the drifting
duplicate this design set out to avoid, and it would be the copy that silently
disagrees at a boundary. The function stays crate-private; no public API grows.

## Automated Test Strategy

Unit tests only, colocated in `#[cfg(test)]` at the bottom of
`src/domain/remediation.rs`. No integration test, because nothing is wired to a
command yet — an integration test would have no surface to exercise. #130 adds
the integration coverage when it renders findings.

Critical path: the three-way classification. Every branch is covered by the real
advisory cases from #133, using their actual specifiers and versions:

| Case | Specifier | Fixed in | Expected |
|---|---|---|---|
| tj-actions/changed-files | `^46` | `46.0.1` | `Upgradable` |
| shivammathur/setup-php | `~2.37` | `2.37.1` | `Upgradable` |
| github/codeql-action | `^2` | `3.0.0` | `OutOfRange` |
| aquasecurity/trivy-action | `^0.34` | `0.35.0` | `OutOfRange` |
| reviewdog/action-setup | `^1` | (none) | `NoFixAvailable` |

Plus: the `v`-prefixed advisory form reaching the same verdict as the bare form;
a branch specifier and a bare-SHA specifier both refusing to suggest an upgrade
(the inverted-polarity case, which is the easiest thing for a later change to
get wrong); and an unparseable identifier landing in `NoFixAvailable`, not
`OutOfRange`.

No new test infrastructure. No fixtures, no network, no fakes — the function is
pure.

## Observability

There are no error paths: the function is total and infallible, returning one of
three variants for every input. Nothing can fail at runtime, so nothing needs to
surface a failure here.

The one silent-failure risk is *semantic*, not operational: a wrong verdict
would be invisible — a missing suggestion looks like "no fix available", and a
wrongly-emitted suggestion only reveals itself when the user runs it during an
incident and nothing changes. That risk is mitigated at the type level (a
non-exhaustive match will not compile in #130) and by test coverage of the real
cases rather than by any runtime signal.

Nothing is logged. Nothing user-visible ships in this change, so there is no
output to observe until #130 renders these variants.

## Risks / Trade-offs

- **The inverted polarity on non-range specifiers is subtle.** A future reader
  may "simplify" `Remediation` to call `matches_version` unconditionally and
  silently start suggesting upgrades for branch pins → mitigated by an explicit
  test for both the branch and bare-SHA specifier cases, and by a doc comment on
  the match arm stating why the answer differs from `matches_version`.
- **Dead code until #130 lands.** The type is `pub` with no in-tree caller →
  accepted; it is the deliverable of #133, and the `pub` visibility plus the
  crate's lint configuration keep it from being flagged. If it proves to attract
  a dead-code warning, the fix is to land #130, not to weaken the lint.
- **The wording of the two no-command messages is not fixed here.** #130 could
  render something less clear than the spec intends → mitigated by the spec
  stating what the user must be told in each case, which #130 is bound by.

## Migration Plan

None. Purely additive; no existing behavior, file format, or output changes.
Rollback is deleting the file and its `pub mod` line.

## Open Questions

None blocking. One deferred to #130: whether an `OutOfRange` finding should name
the next major the user would have to move to. That needs the tag list, which is
a network concern belonging to the audit command, not to this decision.
