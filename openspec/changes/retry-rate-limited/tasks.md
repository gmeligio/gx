## 1. Carry a normalized wait on the rate-limit error

- [x] 1.1 Add a `RetryAfter` enum to `src/domain/resolution.rs` with three variants — `Unstated`, `After(Duration)`, `TooDistant` — each with a doc comment, and add a documented `retry_after: RetryAfter` field to `Error::RateLimited`. Three states, not `Option`: `Unstated` must fall back to backoff while `TooDistant` must stop retrying, so collapsing them makes the distant-reset requirement unimplementable. Keep the diff to this enum and field — other branches touch this file. Do not change the `Display` string. Also correct the now-false `RateLimited` variant doc, which currently says the reset time "is not read from the response" — task 1.5 makes that untrue. It must instead say the reset time is carried as data in `retry_after` while the `Display` string still deliberately omits it, so as not to state a wait to the user that may never be taken.
- [x] 1.2 Update the existing `RateLimited` constructions in `src/domain/resolution.rs` tests and `src/domain/resolution_testutil.rs` to the new shape. Confirm `rate_limited_message_names_forge_and_remedy` (which asserts the message never says "resets") still passes unchanged.
- [x] 1.3 Add `normalize_reset(reset_epoch: u64, now_epoch: u64) -> RetryAfter` in `src/infra/github/registry.rs`: saturating subtraction, returning `TooDistant` when the result exceeds `MAX_RETRY_WAIT` and `Unstated` when the header is absent or unparseable.
- [x] 1.4 Unit-test `normalize_reset` directly: near reset → `After`, one-hour reset → `TooDistant`, past reset (now ahead of reset) → `After(0)` not a negative or panic, unparseable → `Unstated`.
- [x] 1.5 Read `x-ratelimit-reset` in `check_status`, pass it and the current epoch through `normalize_reset`, and populate `retry_after` on both `RateLimited` returns (the 429 path and the 403-with-zero-remaining path). Thread the value through the `github::Error::RateLimited` → `resolution::Error::RateLimited` mappings at all three sites in the `VersionRegistry` impl.

## 2. The retrying decorator

- [ ] 2.1 Create `src/infra/registry/mod.rs` declaring the module, and register `pub mod registry;` in `src/infra/mod.rs`. The new directory holds 2 files, well inside the per-directory file budget that `mise run test` enforces.
- [ ] 2.2 Add the `Waiter` trait and a `ThreadWaiter` production impl wrapping `std::thread::sleep`.
- [ ] 2.3 Implement `Retrying<R, W = ThreadWaiter>` in `src/infra/registry/retrying.rs`: holds the inner registry, a waiter, and an optional `Box<dyn Fn(&str)>` notifier. Constructor `Retrying::new(inner)` uses `ThreadWaiter` and no notifier; a builder method attaches the notifier.
- [ ] 2.4 Implement the shared retry loop: attempt the call, and on an error where `is_retryable()` is true and attempts remain, resolve the wait, announce it via the notifier, sleep, and retry. Constants `MAX_ATTEMPTS = 3`, `MAX_RETRY_WAIT = 5s`, backoff `[1s, 2s]`. Wait resolution by `RetryAfter`: `After(d)` waits exactly `d`; `Unstated` waits the backoff value for this attempt index; `TooDistant` returns the error immediately without waiting or retrying.
- [ ] 2.5 Implement `VersionRegistry for Retrying<R, W>` — `lookup_sha`, `all_tags`, `describe_sha` all route through the shared loop. Keep the three impls thin so the file stays well inside the 440-logic-line budget.
- [ ] 2.6 Verify the type composes as the inner layer: `Retrying<Registry>` must satisfy `VersionRegistry` so a future `Caching::new(Retrying::new(...))` type-checks. Do not create, fetch, or modify any caching file.

## 3. Tests at the decorator seam

- [ ] 3.1 Add a recording `Waiter` fake that stores requested durations and returns instantly, and a scripted `VersionRegistry` fake returning a caller-supplied result sequence while counting calls. Keep both in the retrying module's `#[cfg(test)]` block at file bottom.
- [ ] 3.2 Test: rate-limit then success → `Ok`, exactly 2 calls. (spec: transient limit resolves without user intervention)
- [ ] 3.3 Test: always rate-limited → `Err`, exactly 3 calls. (spec: bounded attempts)
- [ ] 3.4 Test: `AuthRequired` → `Err`, exactly 1 call. (spec: missing credential not retried)
- [ ] 3.5 Test: `ResolveFailed` → `Err`, exactly 1 call. (spec: non-retryable failure not retried)
- [ ] 3.6 Test: `RetryAfter::After(3s)` → the waiter recorded exactly 3s, not a backoff value. (spec: near reset time is waited out)
- [ ] 3.7 Test: `RetryAfter::Unstated` → the waiter recorded `[1s, 2s]`, increasing and each under the cap. (spec: missing reset falls back to increasing backoff)
- [ ] 3.8 Test: `RetryAfter::TooDistant` → `Err`, exactly 1 call, and the waiter recorded nothing. Assert alongside 3.7 so the contrast is pinned — "1 call, no wait" alone is also what a no-op layer produces. (spec: distant reset time is not waited on)
- [ ] 3.9 Test: the notifier fires once per wait and before it, with a message naming the rate limit and the duration. (spec: user sees why the command paused)
- [ ] 3.10 Test: `describe_sha` and `all_tags` each retry a rate-limit error too — 2 calls apiece on rate-limit-then-success. Guards against a trait method bypassing the shared loop.
- [ ] 3.11 Confirm no test sleeps: the retrying module's tests must complete effectively instantly.

## 4. Wire the decorator into the commands

- [ ] 4.1 Wrap the registry in `src/tidy/command.rs` with `Retrying`, attaching a live `Fn(&str)` notifier that reaches `on_progress`. The notice must be emitted *before* the sleep, not drained afterward — a deferred drain leaves the spinner frozen for the whole wait, which is the "reads as a hang" failure the requirement exists to prevent, and it would still pass the decorator-level test in 3.9.
- [ ] 4.2 Wrap the registry in `src/upgrade/command.rs` the same way. Confirm no notifier output reaches stdout in `--json` mode (progress is already suppressed there in `src/main.rs`).
- [ ] 4.3 Wrap the registry in `src/init/command.rs` the same way.
- [ ] 4.4 Confirm `tests/integ_tidy.rs` still passes unchanged — `gx tidy` must still succeed under the always-`AuthRequired` registry, proving retry did not leak into the auth path.

## 5. Vacuity check and gates

- [ ] 5.1 Mutation-test: temporarily set `MAX_ATTEMPTS = 1` and run the suite. Tests 3.2, 3.3, 3.6, 3.7, 3.9, and 3.10 must all fail. Rewrite any that still pass, then restore the constant and re-run. (3.8 is expected to still pass under this mutation — it is pinned by its pairing with 3.7, not by this mutation.)
- [ ] 5.2 Run `mise run test` — must pass. Do not raise any budget in `tests/code_health.rs`; restructure instead.
- [ ] 5.3 Run `mise run integ` — must pass.
- [ ] 5.4 Confirm clippy strict is clean: pedantic, private-item and field docs on every new item, `#[cfg(test)]` at file bottom, any `#[expect(...)]` actually fulfilled.
