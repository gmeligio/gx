## 1. Carry a normalized wait on the rate-limit error

- [ ] 1.1 Add `retry_after: Option<Duration>` to `resolution::Error::RateLimited` in `src/domain/resolution.rs`, with a doc comment on the field. Keep the diff to this field only — other branches touch this file. Do not change the `Display` string.
- [ ] 1.2 Update the existing `RateLimited` constructions in `src/domain/resolution.rs` tests and `src/domain/resolution_testutil.rs` to the new shape. Confirm `rate_limited_message_names_forge_and_remedy` (which asserts the message never says "resets") still passes unchanged.
- [ ] 1.3 Add `normalize_reset(reset_epoch: u64, now_epoch: u64) -> Option<Duration>` in `src/infra/github/registry.rs`: saturating subtraction, returning `None` when the result exceeds `MAX_RETRY_WAIT` and `None` when the header is absent or unparseable.
- [ ] 1.4 Unit-test `normalize_reset` directly: near reset → `Some`, one-hour reset → `None`, past reset (now ahead of reset) → zero-wait not a negative or panic, unparseable → `None`.
- [ ] 1.5 Read `x-ratelimit-reset` in `check_status`, pass it and the current epoch through `normalize_reset`, and populate `retry_after` on both `RateLimited` returns (the 429 path and the 403-with-zero-remaining path). Thread the value through the `github::Error::RateLimited` → `resolution::Error::RateLimited` mappings at all three sites in the `VersionRegistry` impl.

## 2. The retrying decorator

- [ ] 2.1 Create `src/infra/registry/mod.rs` declaring the module, and register `pub mod registry;` in `src/infra/mod.rs`. Re-check `ls src/infra/registry/*.rs | wc -l` stays within the 8-file budget.
- [ ] 2.2 Add the `Waiter` trait and a `ThreadWaiter` production impl wrapping `std::thread::sleep`.
- [ ] 2.3 Implement `Retrying<R, W = ThreadWaiter>` in `src/infra/registry/retrying.rs`: holds the inner registry, a waiter, and an optional `Box<dyn Fn(&str)>` notifier. Constructor `Retrying::new(inner)` uses `ThreadWaiter` and no notifier; a builder method attaches the notifier.
- [ ] 2.4 Implement the shared retry loop: attempt the call, and on an error where `is_retryable()` is true and attempts remain, announce via the notifier, wait, and retry. Constants `MAX_ATTEMPTS = 3`, `MAX_RETRY_WAIT = 5s`, backoff `[1s, 2s]`. Use `retry_after` when `Some`, otherwise the backoff schedule by attempt index.
- [ ] 2.5 Implement `VersionRegistry for Retrying<R, W>` — `lookup_sha`, `all_tags`, `describe_sha` all route through the shared loop. Keep the three impls thin so the file stays well inside the 440-logic-line budget.
- [ ] 2.6 Verify the type composes as the inner layer: `Retrying<Registry>` must satisfy `VersionRegistry` so a future `Caching::new(Retrying::new(...))` type-checks. Do not create, fetch, or modify any caching file.

## 3. Tests at the decorator seam

- [ ] 3.1 Add a recording `Waiter` fake that stores requested durations and returns instantly, and a scripted `VersionRegistry` fake returning a caller-supplied result sequence while counting calls. Keep both in the retrying module's `#[cfg(test)]` block at file bottom.
- [ ] 3.2 Test: rate-limit then success → `Ok`, exactly 2 calls. (spec: transient limit resolves without user intervention)
- [ ] 3.3 Test: always rate-limited → `Err`, exactly 3 calls. (spec: bounded attempts)
- [ ] 3.4 Test: `AuthRequired` → `Err`, exactly 1 call. (spec: missing credential not retried)
- [ ] 3.5 Test: `ResolveFailed` → `Err`, exactly 1 call. (spec: non-retryable failure not retried)
- [ ] 3.6 Test: `retry_after: Some(3s)` → the waiter recorded exactly 3s, not a backoff value. (spec: near reset time is waited out)
- [ ] 3.7 Test: `retry_after: None` → the waiter recorded `[1s, 2s]`, increasing and each under the cap. (spec: missing reset falls back to increasing backoff)
- [ ] 3.8 Test: the notifier fires once per wait and before it, with a message naming the rate limit and the duration. (spec: user sees why the command paused)
- [ ] 3.9 Confirm no test sleeps: the retrying module's tests must complete effectively instantly.

## 4. Wire the decorator into the commands

- [ ] 4.1 Wrap the registry in `src/tidy/command.rs` with `Retrying`, attaching a notifier that reaches `on_progress`. Resolve the borrow conflict as designed — the notifier is `Fn(&str)`, so it may write into a `RefCell` sink the command drains, or forward directly if the borrow permits.
- [ ] 4.2 Wrap the registry in `src/upgrade/command.rs` the same way. Confirm no notifier output reaches stdout in `--json` mode (progress is already suppressed there in `src/main.rs`).
- [ ] 4.3 Wrap the registry in `src/init/command.rs` the same way.
- [ ] 4.4 Confirm `tests/integ_tidy.rs` still passes unchanged — `gx tidy` must still succeed under the always-`AuthRequired` registry, proving retry did not leak into the auth path.

## 5. Vacuity check and gates

- [ ] 5.1 Mutation-test: temporarily set `MAX_ATTEMPTS = 1` and run the suite. Tasks 3.2, 3.3, 3.6, 3.7, and 3.8 must all fail. Rewrite any that still pass, then restore the constant and re-run.
- [ ] 5.2 Run `mise run test` — must pass. Do not raise any budget in `tests/code_health.rs`; restructure instead.
- [ ] 5.3 Run `mise run integ` — must pass.
- [ ] 5.4 Confirm clippy strict is clean: pedantic, private-item and field docs on every new item, `#[cfg(test)]` at file bottom, any `#[expect(...)]` actually fulfilled.
