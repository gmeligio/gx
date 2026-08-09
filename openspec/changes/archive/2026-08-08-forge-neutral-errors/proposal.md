## Why

When `gx tidy` cannot resolve an action, the line the user reads is
`Skipping actions/checkout@^4: GitHub API rate limit exceeded`. That string is built
by `resolution::Error`'s `Display` in `src/domain/resolution.rs` — a layer that
otherwise knows nothing about GitHub. Two things are wrong with it:

1. **It names the wrong thing and helps the user with nothing.** "GitHub API
   authorization required" tells a user *which vendor* failed but not *what to do*.
   The one actionable fact — set `GITHUB_TOKEN` — is missing, and the backend name
   is the part the user could already infer.
2. **It hard-codes the backend into the variant.** A GitLab registry (#145) would
   need `GitLabRateLimited` next to `RateLimited`, growing the enum along two axes
   at once (backend x failure mode) — the shape that becomes a kitchen-sink enum.

Neither `resolution::Error` nor `github::Error` is `#[non_exhaustive]`, so every
future variant is a breaking change. `#[non_exhaustive]` must land *first*: adding
it later is itself the breaking change.

Separately, `is_recoverable()` conflates two different questions that #137 (bounded
retry) is about to pull apart — see below.

## What Changes

- **BREAKING** `resolution::Error` and `github::Error` become `#[non_exhaustive]`,
  so later variants are additive for downstream matchers.
- `resolution::Error::RateLimited` and `AuthRequired` carry the originating forge as
  **data** (a `Forge` field) instead of baking a vendor name into the `Display`
  string. The enum then grows only with failure semantics, never per backend.
- Error messages are rewritten to lead with the actionable remedy. The forge name is
  still rendered — a user with both a GitHub and a GitLab action in one workflow
  needs to know which one ran out of quota — but it comes from the field, not the
  variant.
- `is_recoverable()` is **renamed to `is_skippable()`**. The old name invited a
  retry reading it never supported: its only caller (`tidy::lock_sync`) uses it to
  decide warn-and-skip, and both rate-limit and auth stay `true` there. Reading it
  as "safe to retry" and flipping `AuthRequired` to `false` would hard-fail every
  tokenless run. The name now states the question it answers.

  A caller that retries must decide that from the specific failure, not from
  skippability — the two are different questions, and #137 makes that decision for
  itself.
- `github::Error` keeps its precise, GitHub-specific variants behind the existing
  mapping boundary at `impl VersionRegistry for Registry`, with `#[source]` chaining
  preserved.

Explicitly **not** in scope: retry/backoff (#137), the GitLab registry (#145), and
parsing `X-RateLimit-Reset` into a normalized wait — see design.md for why the wait
is deferred rather than carried speculatively.

## Capabilities

### New Capabilities

None. This adds no capability a user can invoke.

### Modified Capabilities

- `action-resolution`: two requirement-level changes, both user-facing, so the
  relevance gate ("adds, removes, or changes user-facing behavior") is met rather
  than the "internal refactoring with no user-visible change" skip:
  1. The spec's **Guardrail: Error classification** table is the normative statement
     of which failures warn and which fail. Its vocabulary ("recoverable") is the
     name this change removes, and the table must say explicitly that skippable
     carries no retry promise — otherwise the next reader repeats the misreading.
  2. The error text a user reads on a skipped resolution changes. The guardrail's
     "User experience" column already claims territory over that text, and the
     project rule "Error classification determines whether user sees warning or hard
     failure" points at exactly this table.

  Deliberately **not** given a requirement: `#[non_exhaustive]` and the `Forge`
  field's existence. Both are BREAKING at the Rust API level, but `gx` ships as a
  CLI binary with no downstream matchers, so neither is observable to a user. They
  fall under the "internal refactoring with no user-visible change" skip. What *is*
  specified is the consequence a user can see — that adding a forge adds no failure
  variants, and that messages name forge and remedy.

## Impact

- `src/domain/resolution.rs` — `Error` enum, `Forge` type, predicate split, tests.
- `src/infra/github/registry.rs` — `#[non_exhaustive]`; the four `map_err` arms
  construct the forge-carrying variants.
- `src/tidy/lock_sync.rs` — one call site renamed `is_recoverable` → `is_skippable`.
- `src/domain/resolution_testutil.rs`, `src/tidy/command_tests.rs`,
  `src/tidy/lock_sync_tests.rs`, `tests/common/registries.rs` — construct the
  changed variants.
- User-visible strings: the two `resolution::Error` messages. `Event::` text,
  the "No GITHUB_TOKEN set" warnings in `src/init/command.rs` and
  `src/tidy/command.rs`, and all `github::Error` text are unchanged.
- No dependency, CLI-surface, or file-format change.
