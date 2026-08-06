## Context

`VersionRegistry` has three methods (`lookup_sha`, `all_tags`, `describe_sha`) and one real
implementor, `infra::github::Registry`. Against that sit eight hand-written test doubles:

| Double | Location | Purpose |
| --- | --- | --- |
| `MockRegistry` | `src/domain/resolution.rs` (test mod) | canned `Result`s for the two lookups |
| `FakeRegistry` | `src/domain/resolution_testutil.rs` | builder fake, **`describe_sha` ignores the SHA** |
| `AuthRequiredRegistry` | `src/domain/resolution_testutil.rs` | every method → `AuthRequired` |
| `NoopRegistry` | `src/tidy/command_tests.rs` | every method → `AuthRequired` (duplicate of the above) |
| `MixedRegistry` | `src/tidy/lock_sync_tests.rs` | `actions/checkout` fails, others resolve |
| `FakeRegistry` | `tests/common/registries.rs` | builder fake, **`describe_sha` keyed on `(id, sha)`** |
| `AuthRequiredRegistry` | `tests/common/registries.rs` | every method → `AuthRequired` (third copy) |
| `EmptyDateRegistry` | `tests/common/registries.rs` | commit dates are `""` |
| `FailingDescribeRegistry` | `tests/common/registries.rs` | `describe_sha` → `ResolveFailed` |

Three of the eight are one-behavior types (`AuthRequiredRegistry` ×2 / `NoopRegistry`,
`EmptyDateRegistry`, `FailingDescribeRegistry`) — each an entire type where a field would do.

### The contract disagreement

The two `FakeRegistry` types disagree about `describe_sha`:

- `tests/common/registries.rs` stores `sha_tags: HashMap<(id, sha), tags>` and looks up the
  exact `(id, sha)` pair.
- `src/domain/resolution_testutil.rs` stores `tags: HashMap<id, (sha, tags)>` and its
  `describe_sha` binds the SHA parameter as `_sha` — it returns the action's tag list for
  **any** SHA. Its `with_sha_tags(id, sha, tags)` builder takes a `sha` it only uses as
  `lookup_sha`'s return value, never as a `describe_sha` key.

Production settles it. `Registry::describe_sha` calls
`get_tags_for_sha(id.as_str(), sha.as_str())` — the SHA is the lookup key
(`src/infra/github/registry.rs:278`). **The `tests/common` fake is faithful; the `src` fake
is not.**

The unfaithfulness is load-bearing for six tests in `src/tidy/lock_sync_tests.rs`
(lines 106, 226, 264, 302, 341, 381). Each configures tags under SHA
`aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` while feeding `update_lock` a workflow SHA of
`6d1e696000000000000000000000000000000000`. Production would return no tags for that SHA;
the fake returns three. Those tests pass because the fake lies.

## Goals / Non-Goals

**Goals:**
- One configurable in-memory fake, in one place, covering every scenario the eight doubles cover.
- Adding a `VersionRegistry` method means editing exactly one fake.
- Scenarios are **configuration** (builder calls), never new types.
- The fake models the production contract faithfully — including SHA-keyed `describe_sha`.
- The six tests that depended on the unfaithful behavior keep asserting what their names claim.

**Non-Goals:**
- Re-litigating whether `VersionRegistry` should exist at all. The trait having three
  test-only implementors is the concrete-abstraction smell the issue names, but that is a
  separate question and explicitly out of scope.
- Any production behavior change. No file outside test modules and test-only helpers is edited.
- Anticipating scenarios no current test needs. No `RateLimited` knob, no call-counting, no
  latency simulation — none of that has a caller today.
- Reconciling with the `Caching<R>` decorator's counting fake on another branch.

## Decisions

### D1: One fake type, in `src/domain/resolution_testutil.rs`, exported from the library

The fake lives next to the trait it implements and is reachable from both unit tests
(`crate::domain::resolution::testutil`) and integration tests (`gx::domain::resolution::testutil`).

`tests/common/registries.rs` is deleted outright; integration tests import the same fake as
unit tests. That is the whole point — one fake means one import path.

**Constraint:** the module is currently `#[cfg(test)] pub(crate) mod testutil`, so it does
not exist in the library that integration tests link against. It becomes a plain
`#[path = "resolution_testutil.rs"] pub mod testutil` — no `cfg`, no feature.

*Mechanism verified empirically before committing to it.* Two alternatives were tried and
rejected on evidence:

- `#[cfg(any(test, feature = "testutil"))]` + `gx = { path = ".", features = ["testutil"] }`
  in `[dev-dependencies]` (the usual self-dependency trick). **Fails:** the self-dependency
  changes `Cargo.lock`, and both `mise run test` and `mise run integ` invoke
  `cargo test --locked`, which refuses. Making it work would mean editing the shared mise
  task files, which this change has no business doing.
- Keeping two fakes, one per side, unified only within each side. **Rejected:** leaves two
  types named `FakeRegistry` with two contracts — the exact confusion this change exists to
  remove.

**Consequences of the plain `pub mod`, both accepted:**

1. The module is no longer `#[cfg(test)]`, so strict clippy now applies to it as production
   code. A trial promotion surfaced exactly four lint classes, all mechanical: missing field
   docs, `#[must_use]` on builder methods returning `Self`, `#[must_use]` on getters, and a
   `Default` impl. The new fake is written to satisfy them from the start.
2. `Cargo.toml` has `include = ["/src/", ...]`, so the fake ships in the published crate as
   ~200 lines of public API. That is the honest cost of one fake shared by both test tiers.
   It is accepted rather than worked around, because the alternatives above cost more: a
   feature gate cannot be enabled under `--locked`, and duplicating the fake reintroduces the
   defect. Note for a future change: if shipping it becomes unwanted, the fix is a
   `testutil` feature *plus* adding `--features testutil` to the mise test tasks — a
   coordinated edit to shared files that belongs in its own change, not this one.

### D2: `describe_sha` is keyed on `(action, sha)` — the faithful contract wins

The unified fake adopts `tests/common`'s semantics because they match
`Registry::describe_sha`. The `src` fake's "any SHA matches" behavior is deleted.

This is a *finding about the tests*, not a merge preference: unifying in the other direction
would propagate a fake that contradicts production into every test that uses it.

**Consequence:** the six `lock_sync_tests` above must be corrected. Each is re-pointed to
configure its tags under the workflow SHA it actually feeds in
(`6d1e696000000000000000000000000000000000`), so the test exercises the path its name
describes.

The *intent* is that only the fixture SHA changes and every assertion stands as written —
that is the minimal edit that makes each test true again. But that is an **expectation, not
a settled fact**: re-pointing the SHA changes what `describe_sha` returns, which can change
which branch of `resolve_from_sha` executes (`ResolvedRef::Tag` vs `ResolvedRef::Commit`).
Which branch each test actually takes is knowable only by running it. Task 4.2 is the gate
that confirms it, and it is not a formality — if a test lands on the `Commit` branch, it has
been silently rewritten rather than corrected, and gets reported rather than accepted.

*Alternative rejected:* keep a `describe_any_sha()` escape-hatch flag so those six tests
need no edit. That preserves the lie behind a flag and invites the next author to reach for
it. The six tests are wrong today; a flag would make them permanently wrong.

### D3: Scenarios are fields, not types

| Former type | Becomes |
| --- | --- |
| `AuthRequiredRegistry`, `NoopRegistry` | `FakeRegistry::new().failing(Error::AuthRequired { forge })` |
| `EmptyDateRegistry` | `FakeRegistry::new().with_empty_dates()` |
| `FailingDescribeRegistry` | `FakeRegistry::new().failing_describe(reason)` |
| `MixedRegistry` | `FakeRegistry::new().failing_action("actions/checkout", err)` |
| `MockRegistry` | `FakeRegistry::new().with_lookup_result(r)` |

Every knob above has at least one current caller. Nothing speculative is added.

**Verified after implementation.** A caller count over the finished code found
`with_tags_result` (originally planned as `MockRegistry`'s second half) had **zero**
callers — the migrated `MockRegistry` tests all drive `all_tags` through `with_all_tags`
or an error knob instead. It was deleted rather than kept "for symmetry"; a knob with no
caller is exactly the speculation the Non-Goals rule out. Final knob set and caller counts:
`with_all_tags` 13, `with_sha_tags` 10, `failing_auth` 3, `failing_describe` 2,
`with_lookup_result` 2, `failing_action` 1, `with_fixed_sha` 1, `with_empty_dates` 1.

### D4: `lookup_sha`'s default SHA is the deterministic hash

`tests/common`'s `fake_sha(id, version)` produces a 40-hex-char SHA — SHA-*shaped*, which
matters because lock entries and workflow files are compared as SHA strings, and ~30 call
sites already assert against `FakeRegistry::fake_sha(...)`. The `src` fake's fallback of
"the version string" (e.g. a SHA field literally holding `"v4"`) is not SHA-shaped and has
no assertions depending on it.

So: `fake_sha` is the default; `with_fixed_sha(sha)` overrides it globally (used once, by
`version_ref_falls_back_to_registry_resolution`); `with_lookup_result(...)` overrides it with
a full canned `Result` (for `MockRegistry`'s cases).

### D5: One file, watched against budget

`src/domain/` currently holds 6 `.rs` files (budget: 8 non-recursive), so the rewritten
`resolution_testutil.rs` stays where it is and no new file is needed. `src/tidy/` is at 8/8
and is not touched structurally. The unified fake must stay under 440 logic lines; if it
approaches that, the fix is fewer knobs, not a second file.

## Automated Test Strategy

- **Level:** this change *is* test infrastructure; it adds no new tests of its own. The
  existing unit and integration suites are the test of the refactor. `mise run test` and
  `mise run integ` must both pass with no test deleted and no `#[ignore]` added.
- **Critical path:** the six `lock_sync_tests` corrected under D2. They are the only tests
  whose *fixtures* change, so they are the only ones where a silent weakening could hide.
- **New test infrastructure:** the unified `FakeRegistry` itself.
- **Mutation checking (the real gate).** Collapsing doubles can make a test pass for the
  wrong reason — a permissive default turns a real assertion into a tautology. So for a
  representative sample spanning every knob, the migrated test is re-run against a
  deliberately broken fake or production path, and must **fail**:
  1. `sha_first_lock_uses_workflow_sha_and_most_specific_version` — break
     `select_most_specific_tag` to return the *least* specific tag; the test must fail.
     (Proves the SHA-keyed `describe_sha` path is really reached after D2's fixture fix.)
  2. `out_of_range_pinned_sha_is_reresolved_within_range` — make `matches_version` always
     return `true`; the test must fail.
  3. `update_lock_recoverable_errors_are_skipped` — make `is_skippable` return `false`;
     the test must fail.
  4. **`update_lock_recoverable_errors_are_skipped` again, mutating the fake this time** —
     make `failing_action` ignore its action filter (fail *every* action, then fail *none*).
     The test must fail both ways. This is the one knob that models **partial** failure, and
     it is the successor to `MixedRegistry`, so it sits directly on the
     `action-resolution` spec's load-bearing error-classification guardrail ("Mix of
     recoverable and strict errors"). A `failing_action` that quietly fails everything or
     nothing is exactly the permissive-default failure this whole section exists to catch —
     and unlike check 3, which mutates production, this one proves the *fake's* filter is
     real. Task 3.4's "matches the old double" eyeball check is not a substitute.
  5. An `EmptyDateRegistry` successor in `integ_pipeline` — make `with_empty_dates` a no-op;
     the test must fail.
  6. A `FailingDescribeRegistry` successor in `integ_pipeline` — make `failing_describe`
     return `Ok`; the test must fail.
  7. An `all_tags`-driven upgrade test in `integ_upgrade` — make `with_all_tags` drop its
     tags; the test must fail.

  Two knobs are deliberately left unchecked: `with_fixed_sha` (one caller, and its
  assertion compares against the exact SHA it configures, so a broken knob cannot pass) and
  `with_tags_result`. Both are canned-result overrides whose assertions name the canned value
  directly. Recorded here so the omission is a decision rather than an oversight.

  A migrated test that still passes under its mutation is not migrated — it is disabled, and
  gets fixed before this change lands.

## Observability

Failures here surface at compile time or as test failures — this is test-only code with no
runtime user. The specific silent-failure risk is the one D2 exposes: a fake whose defaults
are permissive lets a test pass while asserting nothing. That failure mode is invisible to
`cargo test`, which is exactly why the mutation checks above are the gate rather than a
green suite.

Two design choices reduce it structurally:
- `describe_sha` returns empty tags for an unconfigured SHA rather than falling back to the
  action's full tag list. A test that forgets to configure gets `ResolvedRef::Commit` and a
  visibly wrong version label, not a plausible-looking pass.
- The failure knobs are explicit (`failing`, `failing_describe`, `failing_action`) — the
  default fake never errors, so no test can accidentally inherit an error path.

## Risks / Trade-offs

- **[The six corrected tests might be asserting something else entirely once the fixture is
  fixed]** → Mutation checks 1 and 2 target exactly these. If a corrected test cannot be made
  to fail by breaking the logic it names, it is reported as a pre-existing dead test rather
  than quietly kept.
- **[Exporting `testutil` from the library ships it in the published crate]** → Accepted
  knowingly, not overlooked; see D1 for why both alternatives cost more under `--locked`.
  The fake has no dependencies beyond `std` and the crate's own domain types, so it adds no
  transitive weight — only public surface. **This is a known non-blocking problem and should
  outlive this change as a tracked issue**, since a note in a document that gets archived is
  not a durable record. Filing is left to the maintainer (this change is under instruction
  not to open issues); it is surfaced in the completion report so it is not lost.
- **[Promoting the module out of `#[cfg(test)]` subjects it to strict clippy]** → Confirmed
  by trial promotion: four mechanical lint classes, no design change needed. Written to
  comply from the start rather than fixed after the fact.
- **[One fake with many knobs becomes a god-object]** → Six knobs, each with a current
  caller, in a file well under the 440-line budget. The budget check in `tests/code_health.rs`
  enforces the ceiling; no budget number is raised.
- **[Concurrent `Caching<R>` work has its own counting fake]** → Out of scope by instruction.
  Noted at merge; the unified fake would likely serve it, since a decorator test needs the
  inner registry to be configurable in exactly these ways.

## Migration Plan

Single commit, test-only, no rollback concern — reverting is `git revert`. No deploy step,
no data migration, no user-facing surface.

## Open Questions

None blocking. The one judgment call — which `describe_sha` contract is correct — is settled
by production code (D2) rather than left open.
