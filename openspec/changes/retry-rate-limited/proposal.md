## Why

Hitting the GitHub rate limit fails an action outright. An unauthenticated user (60 requests/hour) gets that action skipped from the lock even when the quota would have reset seconds later, and the only remedy is to notice the warning and rerun by hand. A short, bounded wait recovers the common case — a quota window that is about to roll over — without the user doing anything.

The domain already classifies rate limiting as retryable (`Error::is_retryable()`), but nothing acts on that signal: there is no retry, no backoff, and no `sleep` anywhere in `src/`. This change is the first and intended consumer of that predicate.

## What Changes

- A retrying decorator over `VersionRegistry` re-issues a request that failed with `RateLimited`, up to a small fixed bound, waiting between attempts. All three commands that talk to GitHub (`gx init`, `gx tidy`, `gx upgrade`) get it.
- Only `RateLimited` is retried. `AuthRequired` is skippable but never retryable — reissuing a request with the same absent credential cannot succeed — and every other error fails immediately, as today.
- The GitHub `X-RateLimit-Reset` header (absolute epoch seconds) is read and carried on `RateLimited` as a **normalized, already-clamped wait duration**, so the retry can sleep exactly as long as the quota actually needs instead of guessing. Today this header is not read at all.
- The wait is capped. An exhausted unauthenticated quota can reset up to an hour out; blocking that long is worse than failing, so a reset beyond the cap is not waited on — the error surfaces immediately and the action is skipped as it is today.
- Each wait is announced through the existing progress channel, so a multi-second stall is never silent. In `--json` mode, where the spinner and log are suppressed to keep stdout a single document, the retry stays silent on stdout and the outcome is visible only in the final report — unchanged from today.
- **No new configuration surface.** Retry counts and caps are fixed constants chosen to be safe defaults.

## Capabilities

### New Capabilities

None. Retrying is not a new domain concept — it is a refinement of how an already-classified error condition is handled.

### Modified Capabilities

- `action-resolution`: the "Error classification" guardrail currently sends a rate-limited action straight to warn-and-skip. It gains a preceding step — a bounded automatic retry — and the requirement that the wait is bounded, announced, and derived from the forge's stated reset time when that time is near enough to be worth waiting for.

## Impact

- **New**: `src/infra/registry/` module holding the retrying decorator and an injectable clock/sleeper so tests never actually sleep.
- **Modified**: `src/domain/resolution.rs` — `RateLimited` gains a `retry_after` field (the enum is `#[non_exhaustive]` with named fields, so this is non-breaking). `src/infra/github/registry.rs` — `check_status` reads `X-RateLimit-Reset` and normalizes it into that field.
- **Modified**: `src/init/command.rs`, `src/tidy/command.rs`, `src/upgrade/command.rs` — wrap the constructed registry in the decorator.
- **Composition**: the decorator is built to sit *inside* a caching decorator (`Caching::new(Retrying::new(...))`) so a cache hit short-circuits before any retry runs.
- **No new dependencies.** Uses `std::thread::sleep` and `std::time`.
