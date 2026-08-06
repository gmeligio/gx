## Context

`src/domain/resolution.rs` defines the port (`VersionRegistry`) and the error type
every registry implementation maps into. `src/infra/github/registry.rs` is the only
implementation; it maps its own precise `github::Error` into `resolution::Error` in
four `map_err` blocks. The layering is right — only the rendered strings and the
classification predicate leak.

Two consumers constrain the work:

- `src/tidy/lock_sync.rs:52` calls `is_recoverable()` to decide warn-and-skip vs.
  fail. The skipped error's `Display` is stored on `Event::ResolutionSkipped` and
  printed verbatim, so **`resolution::Error`'s message text is user-visible output**.
- #137 (bounded retry) will gate retries on the same predicate, and #145 adds a
  second forge.

`tests/integ_tidy.rs:674` asserts `gx tidy` *succeeds* against a registry that
always returns `AuthRequired`. That test encodes a real UX promise: a user with no
token gets a partial lock and a warning, not a hard failure.

## Goals / Non-Goals

**Goals:**

- `resolution::Error` names no vendor in any variant identifier or literal string.
- Both error enums are `#[non_exhaustive]` before any variant is added.
- Adding a forge adds zero failure variants.
- A caller can ask "may I skip this?" and "may I retry this?" and get correct,
  separate answers.
- Messages tell the user what to do.

**Non-Goals:**

- Retry, backoff, or sleeping (#137).
- A GitLab registry (#145).
- Parsing `X-RateLimit-Reset` / `Retry-After` into a normalized wait — deferred, see
  Decision 4.
- Any change to `github::Error`'s variants or messages beyond `#[non_exhaustive]`.

## Decisions

### Decision 1: Carry the forge as a field on a `Forge` enum, not a string

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Forge { GitHub }

impl fmt::Display for Forge { /* "GitHub" */ }
```

`RateLimited { forge: Forge }` and `AuthRequired { forge: Forge }`.

*Why an enum over `&'static str`:* the set of forges is closed and known at compile
time, and each forge needs an associated remedy string (`GITHUB_TOKEN` vs.
`GITLAB_TOKEN`). An enum keeps that mapping exhaustively checked; a string would let
a typo through and force the remedy to be passed separately at every construction
site. `Copy` keeps the four `map_err` arms free of clones.

*Why not a generic parameter* (`Error<F: Forge>`): it would infect `VersionRegistry`,
every caller, and `TidyError` with a type parameter to express something only the
`Display` impl reads. Rejected as disproportionate.

*Where it lives:* `src/domain/resolution.rs`, beside the error that uses it. The
file-count budget does not force this — `src/domain/` holds 6 `.rs` files against a
budget of 8 (`tests/code_health.rs`), so a `forge.rs` would fit. It is co-located
because the type is ~15 lines and has exactly one consumer; a file per small type
would fragment the error definition across two places for no reader benefit.

### Decision 2: Split `is_recoverable()` into `is_skippable()` and `is_retryable()`

The prompt's premise — "retrying a missing token never succeeds, so `AuthRequired`
is not recoverable" — is correct about *retry* and wrong about the predicate's
current meaning. `is_recoverable()` today gates warn-and-skip, where `AuthRequired`
must stay `true`: flipping one bit would make `gx tidy` hard-fail for every user
without a `GITHUB_TOKEN`, contradicting the spec guardrail and breaking
`tests/integ_tidy.rs:674`. That is a UX regression, not a fix.

The real defect is that one predicate answers two questions. So:

| variant | `is_skippable` | `is_retryable` |
|---|---|---|
| `RateLimited` | true | true |
| `AuthRequired` | true | **false** |
| `ResolveFailed` | false | false |
| `NoTagsForSha` | false | false |

`is_recoverable()` is **renamed** to `is_skippable()` rather than kept as an alias:
its one caller is renamed with it, the name now says what it decides, and leaving
both would preserve exactly the ambiguity being removed. `is_retryable()` is added
with no caller yet — it is the single piece of forward work this change owes #137,
and it is ~4 lines, fully tested, and impossible to get right later without
re-deciding the classification. Adding it here is what prevents #137 from wiring
retry to the wrong bit.

*Why this is not the same "no consumer yet" that Decision 4 rejects.* Decision 4
declines `retry_after` because it has no consumer; `is_retryable()` is added despite
having none. The distinguishing test is not "does a caller exist" but "can this be
got right later at the same cost". The predicate is a pure function of the variant
set being decided in this very change — deferring it means #137 re-opens the
classification with the enum already frozen, and the wrong-answer case
(`AuthRequired`) is exactly the one a retry author is likely to get wrong. The
`Duration` is the opposite: its correct value depends on clock and skew policy that
belong with the retry loop, so writing it now would be guessing at a shape #137
would change anyway.

*Alternative considered — a `Classification` enum returned by one method*
(`Skip | Retry | Fail`). Rejected: it models the two properties as mutually
exclusive, but `RateLimited` is genuinely both, so the enum would need
`SkipAndRetry` and collapse back into a product of two booleans.

### Decision 3: Message text leads with the remedy

Current, and what a user actually sees on the skip line:

```
Skipping actions/checkout@^4: GitHub API rate limit exceeded
Skipping actions/checkout@^4: GitHub API authorization required
```

New:

```
Skipping actions/checkout@^4: GitHub rate limit exhausted; set GITHUB_TOKEN to raise the limit
Skipping actions/checkout@^4: GitHub requires authorization; set GITHUB_TOKEN to a token with repository read access
```

The rate-limit message deliberately does *not* say when the limit resets. Decision 4
declines to parse `X-RateLimit-Reset`, so the code does not know — under the
unauthenticated 60/hour limit the true answer can be most of an hour, and "resets
shortly" would be a guess presented as fact. The message states only what is known
and what the user controls. When #137 parses the reset into a real wait, the message
can state it accurately.

The vendor word survives — but as `{forge}` interpolated from the field, not as a
literal in a vendor-specific variant. That is the distinction the issue draws:
the *enum* stops growing per backend; the *message* still tells a user with actions
from two forges which one failed. A message that said only "rate limit exhausted"
would be forge-neutral and less useful, which fails the planning constraint that the
message must say what went wrong and what to do.

The remedy text is per-forge data (`GITHUB_TOKEN` for `Forge::GitHub`), supplied by
a `Forge::token_env()` accessor, so a second forge supplies its own without touching
the variants.

*Deliberately unchanged:* the pre-run `"Warning: No GITHUB_TOKEN set — using
unauthenticated GitHub API (60 requests/hour limit)."` in `src/init/command.rs` and
`src/tidy/command.rs`. It is emitted before any request, is keyed off configuration
rather than a response, and already names its remedy. Rewording it is out of scope
and would churn integration assertions for no gain.

### Decision 4: Do NOT carry a normalized wait on `RateLimited` yet

Tempting, since #137 needs it and the header parsing is genuine normalization work
(GitHub: absolute `X-RateLimit-Reset`; GitLab: relative `Retry-After`). Deferred
anyway:

- No consumer exists. `Duration` on the variant would be constructed, rendered
  never, and read by nobody until #137 lands — speculative by the project's own
  "minimum code that solves the problem" rule.
- It cannot be produced correctly here. `check_status` receives `&Response`, but
  `get_json` calls it only on the non-success path and the headers needed for a
  *sound* wait (`X-RateLimit-Reset` is absolute epoch seconds, requiring a clock and
  a skew policy) bring in decisions — clamp, floor, what to do when absent — that
  belong with the retry loop that consumes them. Guessing them now means #137
  changes the shape anyway.
- The extension point is free. `#[non_exhaustive]` on the enum plus a struct-shaped
  variant (`RateLimited { forge }`, not a tuple) means #137 adds
  `retry_after: Option<Duration>` as a **field addition**, not a breaking redesign.

So this change *earns* the right to defer it: the deferral costs nothing precisely
because the `#[non_exhaustive]` + named-field shape is being established now.

### Decision 5: `#[non_exhaustive]` on both enums, wildcard arms only where forced

`github::Error` gains `#[non_exhaustive]`. Its four `map_err` sites in the same
crate list variants exhaustively today; `#[non_exhaustive]` does not force a
wildcard for same-crate matches, so those arms stay exhaustive and keep telling us
at compile time when a new variant needs mapping. That property is worth preserving
and is the reason not to pre-emptively add `_ =>` arms.

## Automated Test Strategy

Unit-level, in the existing `#[cfg(test)]` block at the bottom of
`src/domain/resolution.rs`. No new test infrastructure.

- **Critical path:** the classification table. One test per variant per predicate,
  with `is_retryable()` on `AuthRequired` being the assertion that exists to stop
  #137 from retrying an unsatisfiable request. These are pure and cheap.
- **Message text:** assert the rendered `Display` of each forge-carrying variant
  contains the forge name and the remedy env var. Asserting the substring rather
  than the full string keeps the test from being a change-detector on wording while
  still pinning the two facts the spec requires.
- **Regression guard:** `tests/integ_tidy.rs:674` (`tidy` succeeds under
  `AuthRequiredRegistry`) is the executable form of the skippable promise and must
  keep passing untouched. If it fails, the classification was got wrong.
- Existing tests in `src/tidy/lock_sync_tests.rs`, `src/tidy/command_tests.rs`,
  `src/domain/resolution_testutil.rs`, and `tests/common/registries.rs` are updated
  mechanically for the new variant shape — they assert behavior, not message text,
  so none of their assertions change meaning.

## Observability

Resolution failures surface on exactly one path: `lock_sync` classifies the error,
pushes `Event::ResolutionSkipped { reason: e.to_string() }` for skippable ones, and
the command orchestrator prints each event. A trailing
`Event::RecoverableWarning { count }` tells the user how many were skipped and to
re-run. Strict errors become `TidyError::ResolutionFailed` and exit non-zero.

**Can a failure be silent?** A skipped action is never silent — it prints a line and
is counted. The one thing that is quiet by design is `describe_sha`'s tag lookup,
which swallows errors and returns empty tags (`registry.rs:281`); that predates this
change and is unaffected by it.

**What this change improves:** the printed reason now names the remedy, so the
warning is actionable rather than merely descriptive. What it does not change is
*when* a line is printed — the skippable set is identical before and after, which is
the property that keeps this from being a behavioral regression.

## Risks / Trade-offs

- **[Renaming `is_recoverable` is a breaking API change on a public method]** → The
  crate is a CLI binary; the method has one in-tree caller. `#[non_exhaustive]` in
  the same change is the larger break and is deliberate. Both are cheap now and
  expensive after #137 and #145 build on the current shape.
- **[Two predicates invite a caller using the wrong one]** → The names state the
  question they answer, and each has a doc comment giving the wrong-answer case
  (`is_retryable` explicitly notes auth is excluded because retry cannot help).
- **[Message wording churn could break integration assertions]** → Searched: no test
  asserts on the two changed strings. `tests/integ_tidy.rs` asserts on exit status
  and lock contents. `mise run integ` confirms.
- **[`Forge` has a single variant today and looks like over-abstraction]** → It is
  the field that lets the enum stop growing per backend, which is the issue's whole
  point; #145 adds its second variant. A single-variant enum with a `Display` and a
  `token_env()` accessor is ~15 lines and carries the per-forge remedy that Decision
  3 needs regardless.
- **[Merge conflict with the concurrent #136 work in `resolution.rs`]** → This diff
  is confined to the `Error` enum, the predicates, and their tests; #136 touches
  `resolve_from_sha`'s signature and `ShaIndex`. Different regions of the file.

## Migration Plan

Single commit, no data migration, no config change. Rollback is a revert; nothing
persists error text to disk (`gx.lock` stores resolved entries, never errors).

## Open Questions

None blocking. The one judgment call deliberately settled rather than left open is
Decision 4 (defer the normalized wait), because leaving it open would push the
decision into #137 after the enum shape is fixed.
