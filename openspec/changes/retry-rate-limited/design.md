## Context

`VersionRegistry` (`src/domain/resolution.rs`) is the domain's port for asking a forge about versions and SHAs. `github::Registry` (`src/infra/github/registry.rs`) is its only production adapter. `check_status` there already classifies a 429, and a 403 with `x-ratelimit-remaining: 0`, into `Error::RateLimited` — which the domain maps to `resolution::Error::RateLimited { forge }`.

Three commands construct the registry: `src/init/command.rs:53`, `src/tidy/command.rs:211`, `src/upgrade/command.rs:91`. Each has an `on_progress: &mut dyn FnMut(&str)` in scope at that point, but hands it onward to `plan(...)` — so it is mutably borrowed for the duration of the resolution work and cannot simply be handed to a long-lived registry as well.

`resolution::Error::is_retryable()` exists and returns `true` only for `RateLimited`. It has no caller today. `is_skippable()` is a different question answered in `src/tidy/lock_sync.rs`, where a skippable error becomes a warning and the lock is written without that entry; `tests/integ_tidy.rs:674` asserts `gx tidy` *succeeds* against an always-`AuthRequired` registry. Nothing may change that.

There is no `sleep` anywhere in `src/`. The only `Duration` values are a 30s HTTP request timeout and an 80ms spinner tick.

Constraint from a concurrent, unmerged branch: a `Caching<R>` decorator over `VersionRegistry` is arriving at `src/infra/registry/caching.rs`. The intended composition is `Caching::new(Retrying::new(Registry::new(token)?))` so a cache hit short-circuits before any retry runs. This design must compose that way and must not assume the caching file exists.

## Goals / Non-Goals

**Goals:**

- Recover automatically from a rate-limit window that is about to roll over, without the user rerunning anything.
- Stay bounded in both attempts and wall-clock time, so an exhausted quota fails fast.
- Never retry an error where repeating the request cannot change the outcome.
- Never stall silently — a multi-second pause must be explained.
- Keep the test suite fast: no test may actually sleep.

**Non-Goals:**

- No configuration surface for retry counts, caps, or backoff. Fixed constants only. A knob here would be speculative; nobody has asked to tune it, and a wrong value produces a hang, which is exactly what the caps exist to prevent.
- No retry for network/transport errors (connection reset, timeout). Those are classified `ResolveFailed` today and stay strict failures; widening retry to them is a separate judgement about a different failure mode.
- No change to skip/warn behavior after the budget is spent — the existing path handles it.
- No async. The registry is `reqwest::blocking`.

## Decisions

### Decision 1: A `Retrying<R>` decorator in a new `src/infra/registry/` module

The retry logic is forge-agnostic — it keys entirely off `resolution::Error::is_retryable()`, which is already forge-neutral. Putting it beside `github` rather than inside it says so, and it is the directory the incoming `Caching<R>` already claims.

**Alternative rejected — retry inside `get_json`:** it would live at the HTTP layer where the reset header is read, which is tempting. But two call sites are deliberately exempt from `get_json` (`dereference_tag`, which swallows errors into `Option`, and `get_version_tags`, which must read the `Link` header before `json()` consumes the response), so a retry there would cover only some requests. It would also be untestable without a live HTTP server. At the `VersionRegistry` seam every path is covered uniformly and a fake registry makes it trivially testable.

**Alternative rejected — retry inside `ActionResolver`:** the resolver is domain logic with no business knowing about wall-clock waits, and it would not cover `all_tags`/`describe_sha` calls made outside it.

### Decision 2: Carry a normalized, pre-clamped wait on `RateLimited`

`resolution::Error::RateLimited` gains one field:

```rust
RateLimited {
    forge: Forge,
    /// What the forge said about when its quota resets, already normalized
    /// against the local clock and clamped.
    retry_after: RetryAfter,
}

/// The forge's stated reset time, reduced to the three cases the retry layer
/// must tell apart.
pub enum RetryAfter {
    /// No reset time was stated (or it was unparseable) — use the backoff schedule.
    Unstated,
    /// A reset near enough to wait out.
    After(Duration),
    /// A reset further out than any acceptable wait — do not retry at all.
    TooDistant,
}
```

**Three states, not two.** This is the correction the spec forces: "reset stated but an hour out" and "no reset stated" demand *opposite* responses. An unstated reset means fall back to backoff and keep trying. A distant reset means stop — the spec requires "no wait occurs and the rate-limit error is returned immediately," because an exhausted unauthenticated quota reports exactly such a distant reset, and backing off 1s + 2s per action across a whole manifest would burn a minute or more to arrive at the same warn-and-skip the user could have had instantly. Collapsing these into a single `Option::None` would make the distant-reset requirement unimplementable.

The enum is `#[non_exhaustive]` with named fields, so this is non-breaking. This keeps the diff in `resolution.rs` to one field plus its doc, as required.

The critical choice is *where* the absolute epoch timestamp becomes a duration. It is normalized **at the boundary, in `github::registry`**, not carried raw into the domain. Three reasons:

1. `X-RateLimit-Reset` being absolute epoch seconds is a GitHub wire detail. The domain should not learn it.
2. Clock skew is measured against the moment the response arrived. Normalizing later, after retries and waits have elapsed, would compare a stale timestamp to a moved clock.
3. It keeps `resolution.rs` — a file other branches touch — free of any time arithmetic.

Normalization is saturating: `reset_epoch.saturating_sub(now_epoch)`. A reset time in the past (local clock ahead of GitHub's) yields zero, not a negative or a panic. `arithmetic_side_effects` is denied project-wide, so saturating arithmetic is mandatory anyway.

Clamping also happens at normalization: a computed wait exceeding `MAX_RETRY_WAIT` yields `TooDistant`, never a long duration. The retry layer therefore never needs to re-check the cap — every `After(d)` it receives is already within bounds, and the backoff schedule's own values are all under the cap by construction.

Note this changes an existing assertion: `rate_limited_message_names_forge_and_remedy` asserts the message must not contain "resets". The `Display` string stays as it is — the wait is data for the retry layer, not user-facing text — so that assertion continues to hold and continues to be meaningful.

### Decision 3: Bounds — 3 attempts total, 5s cap per wait, backoff 1s → 2s

- **`MAX_ATTEMPTS = 3`** (the initial request plus 2 retries). With the `[1s, 2s]` backoff below, that means a request whose reset is unstated gets up to 3s of waiting before giving up. Two retries covers a quota window that ticks over within a few seconds; a third would grow worst-case latency for no meaningful additional recovery, since a limit that has not lifted within 3s and stated no reset time is not lifting on this run's timescale.
- **`MAX_RETRY_WAIT = 5s`.** The bound that matters most. GitHub's unauthenticated reset can be nearly an hour out; the spec is explicit that not waiting beats stalling. 5s is chosen against the 30s HTTP timeout already in the client — a user tolerating a 30s request tolerates a 5s pause, so this adds nothing qualitatively new to worst-case latency.
- **Backoff `[1s, 2s]`**, used only when no usable reset time was stated. Increasing, and both values under the cap.

Worst case added latency per action: 3s of sleeping. Worst case for a whole run is bounded by the number of actions, but in practice the first action to exhaust the budget means the quota is genuinely gone and every subsequent action pays the same 3s. That is the argued weak point of this design; see Risks.

### Decision 4: An injectable `Waiter` so tests never sleep

```rust
pub trait Waiter {
    fn wait(&self, duration: Duration);
}
```

Production uses a `ThreadWaiter` calling `std::thread::sleep`. Tests use a recording fake that stores the requested durations and returns instantly, which makes the wait schedule directly *assertable* rather than merely fast — a test can prove the reset time was honored and the cap applied, which a real sleep could only prove by being slow.

`Retrying<R, W: Waiter>` is generic over it with `ThreadWaiter` as the default type parameter, so production call sites stay `Retrying::new(registry)`.

**Alternative rejected — a full `Clock` trait:** the only clock reading is `now` during normalization, inside `github::registry`. Faking it would require threading a clock into the HTTP adapter to test one `saturating_sub`. That subtraction is instead extracted as a pure function `normalize_reset(reset_epoch, now_epoch) -> RetryAfter` and unit-tested directly with both operands as arguments — no trait, no injection, full coverage of the skew and cap cases.

### Decision 5: Announce a wait through a shared `RefCell` notifier, not the `&mut` callback

`VersionRegistry::lookup_sha` takes `&self`, and `on_progress` is already mutably borrowed by `plan(...)` at every construction site, so the decorator cannot hold `&mut dyn FnMut`.

**Chosen:** the decorator holds `Option<Box<dyn Fn(&str)>>`, supplied at construction and invoked *immediately before* each sleep. `Fn` — not `FnMut` — is what sidesteps the borrow conflict, because it needs only `&self`. The command passes a closure writing into a `RefCell`-wrapped sink that it also reads; `RefCell` is already used in this codebase (`src/infra/shellcheck/fake.rs:13`). The message is emitted live, before the process sleeps, which is what the spec requires.

**Alternative rejected — a `RefCell<Vec<String>>` of notices drained by the command after planning.** It is simpler and avoids the closure entirely, but the notice would appear *after* the wait rather than during it — leaving the spinner frozen and unexplained for the whole sleep, which is precisely the "reads as a hang" failure the announcement requirement exists to prevent. Task 4.1 must take the live-notifier branch; a deferred drain would satisfy the decorator-level test while still delivering the bad experience.

In `--json` mode the spinner and log file are already suppressed (`src/main.rs:194-199`) precisely so stdout stays one document; the notifier flows through the same suppressed channel and so writes nothing to stdout. No `--json`-specific branch is needed in the retry layer.

## Automated Test Strategy

**Level: unit, at the decorator seam.** The retry layer's entire contract is expressible against a fake `VersionRegistry` and a fake `Waiter`. No HTTP, no network, no sleeping. This is the critical path and gets the bulk of coverage.

New test infrastructure: a scripted fake registry that returns a caller-supplied sequence of results and counts calls, plus the recording `Waiter`. The existing `FakeRegistry` (`src/domain/resolution_testutil.rs`) is built for resolution scenarios rather than call-sequence scripting; check at implementation time whether it can be reused before adding a second fake, since task 1.2 already touches that file.

Critical path cases, each mapped to a spec scenario:

| Test | Proves | Fails if retry reverted? |
|---|---|---|
| Rate-limit then success returns Ok, 2 calls | Transient limit recovers | Yes — returns Err, 1 call |
| Always rate-limited returns Err after exactly 3 calls | Budget is bounded *and* nonzero | Yes — 1 call, not 3 |
| `AuthRequired` returns Err after exactly 1 call | Auth never retried | No by itself — pairs with the above; a no-op layer also gives 1 call. Its value is guarding against over-broad retry, so it is asserted alongside the always-rate-limited test which pins the contrast |
| `ResolveFailed` returns Err after exactly 1 call | Strict errors never retried | Same pairing rationale |
| `After(3s)` → waiter recorded exactly 3s | Reset time honored, not backoff | Yes — no wait recorded at all |
| `Unstated` → waiter recorded `[1s, 2s]` | Backoff schedule, increasing | Yes — no waits recorded |
| `TooDistant` → `Err`, exactly 1 call, no wait recorded | Distant reset stops rather than backs off | Yes — but it is the `Unstated` row that makes this non-vacuous: without it, "1 call, no wait" is also what a no-op layer does. The pair together pin the contrast |
| Notifier invoked once per wait, before it | Wait is announced | Yes — never invoked |
| `describe_sha` and `all_tags` retry too | Every trait method routes through the loop | Yes — 1 call each, not 2 |

Plus pure-function unit tests for `normalize_reset`: near reset → `After`, hour-out reset → `TooDistant`, past reset (now ahead of reset) → `After(0)`, absent or unparseable header → `Unstated`.

**Mutation check (required):** before finalizing, `MAX_ATTEMPTS` is temporarily set to 1 and the suite run. The transient-recovery test, the bounded-budget test, both wait-schedule tests, the notifier test, and the per-method test must all fail. Any test that still passes — other than the two explicitly noted above as pinned by pairing rather than by this mutation — is vacuous and gets rewritten. This is recorded as an explicit task.

**Deliberately not specified:** a `--json` scenario asserting stdout stays one document. Progress is already suppressed in `--json` mode before this change, so such a scenario would pass identically with and without the retry layer. Asserting it would add a green test that proves nothing about this change.

**Integration:** `tests/integ_tidy.rs` already asserts `gx tidy` succeeds under an always-`AuthRequired` registry. That test must keep passing unchanged — it is the regression guard proving retry did not leak into the auth path. No new integration test is added; the retry seam is not reachable from the integration harness without a fake HTTP server, which is disproportionate for this change.

## Observability

Failures and pauses surface through three channels, none silent:

- **The pause itself** is announced through `on_progress` before the process sleeps: a message naming the rate limit and the wait duration. On a TTY this becomes the spinner message; in CI it becomes a timestamped stdout line; with a log file it is written there. This is the single most important observability decision in the change — an unannounced multi-second stall is indistinguishable from a hang, which is a worse user experience than the outright failure this change replaces.
- **Exhausting the budget** is not a new error path. The same `RateLimited` error surfaces to `lock_sync.rs`, becomes a `ResolutionSkipped` event and a `RecoverableWarning`, and appears in the rendered report exactly as today. The user sees no new failure mode — only, sometimes, a successful resolution where they previously saw a warning.
- **`--json` mode**: progress is already suppressed there, so retries are invisible on stdout by construction and the document stays valid. The consequence — a `gx upgrade --json` run can take seconds longer with no indication why — is accepted, because emitting anything to stdout would corrupt the contract, and stderr progress in an unattended context has no reader.

**Can a failure be silent?** One case: the notifier is optional (`Option<Box<dyn Fn(&str)>>`) so a registry constructed without one retries silently. That is the correct default for a library-style construction with no user attached, but it means a missed wiring at a call site produces exactly the silent stall this section exists to prevent. Mitigated by a task that explicitly wires all three call sites, and by the notifier test.

## Risks / Trade-offs

- **An exhausted quota could cost sleep on every remaining action, where it previously failed instantly.** With 30 unresolved actions, a naive 3s-per-action backoff would burn 90 seconds to reach the same warn-and-skip the user gets instantly today — a regression for the exact unauthenticated user the proposal names. → Largely neutralized by the `TooDistant` state: a genuinely exhausted GitHub quota *states* a distant reset, which now short-circuits to an immediate error with no wait at all. The residual exposure is a forge that returns a rate limit with no reset header on every action, which costs 3s each. That case is not GitHub's documented behavior, so a run-scoped circuit breaker ("the quota is gone, stop trying") is deliberately not built now — it would be real added state for a case the `TooDistant` short-circuit already covers in practice. Flagged as the follow-up if it bites.
- **Clock skew between the local machine and GitHub.** → Handled by saturating arithmetic (never negative) and the cap (never long). A skewed-fast clock under-waits and burns a retry; a skewed-slow clock is clamped. Neither produces incorrect results, only a wasted attempt.
- **`RefCell` notifier could panic on re-entrant borrow.** → The notifier is `Fn(&str)` and invoked at exactly one place with no re-entry into the registry, so no borrow is held across a call. Low risk, but the reason it is `Fn` and not `FnMut`.
- **Merge conflict with the incoming `Caching<R>` on `src/infra/registry/mod.rs`.** → Unavoidable; the module declaration is one line and trivially resolvable. The decorator is written to compose as the inner layer as specified.

## Migration Plan

None required. Additive behavior change with no data format, config, or CLI surface change. Rollback is reverting the commit; the pre-existing warn-and-skip path is untouched and remains the terminal behavior.

## Open Questions

None blocking. The circuit-breaker question under Risks is deliberately deferred rather than open — the decision is to not build it now.
