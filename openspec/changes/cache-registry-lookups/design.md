## Context

`VersionRegistry` (`src/domain/resolution.rs`) is the port through which every registry query flows: `lookup_sha`, `all_tags`, `describe_sha`. Every method takes `&self`.

(This change was written when the port also carried a fourth method, `tags_for_sha`. It had no production caller, and #141 deleted it — see the scope note under Decision 1.)

Today exactly one of those is deduplicated. `ShaIndex` (`src/domain/action/tag_selection.rs`) is a `HashMap<(ActionId, CommitSha), ShaDescription>` threaded as `&mut` through four production files and roughly 24 test sites, to memoize `describe_sha` alone. `ActionResolver::resolve_from_sha` carries `sha_index: &mut ShaIndex` as a third parameter — a domain service whose signature is shaped by a caching concern.

The method that repeats hardest cannot use that mechanism at all. `all_tags` is called in a loop over every manifest spec at `src/upgrade/plan.rs:222` (and again at `:163`) through `service.registry()`, an accessor that reaches past `ActionResolver` to the bare registry. There is no `&mut` in scope there, so covering `all_tags` with the existing idiom would mean a second round of `&mut` threading through `determine_upgrades` → `plan`.

Constraints: single-threaded blocking `reqwest`; `src/domain/action/` is at its 8-file directory budget; the eight existing `VersionRegistry` test doubles should not have to learn about caching; issue #137 will add a retry layer at the same boundary.

## Goals / Non-Goals

**Goals:**

- Deduplicate every registry method for the duration of one command run.
- Cover `all_tags` at `src/upgrade/plan.rs:222` with zero call-site changes.
- Remove the `&mut ShaIndex` threading rather than extend it — net deletion.
- Leave a composition boundary that #137's retry layer can nest inside without redesign.

**Non-Goals:**

- No TTL, no eviction, no size bound, no disk persistence. The cache lives and dies with the process.
- No configuration surface — no flag, no env var, no manifest key. Caching is unconditional.
- No thread safety. The codebase is single-threaded and nothing here should suggest otherwise.
- No caching of errors.
- No change to `select_most_specific_tag` or `parse_version_components`, which share `tag_selection.rs` with `ShaIndex` but are unrelated to caching and stay.

## Decisions

### Decision 1: A decorator that implements the port, not an extension of `ShaIndex`

`Caching<R: VersionRegistry>` implements `VersionRegistry` by delegating to an inner `R` and memoizing each result. It is constructed at each command's composition root:

```rust
let registry = Caching::new(GithubRegistry::new(token)?);
```

*Why over extending `ShaIndex`:* the `&mut` threading has already metastasized — 40 lines of cache paid for with ceremony across four production files and ~24 test sites, to cover one of four methods. Extending it to a second method doubles that cost, and the hottest method (`all_tags`) is structurally unreachable from it. A decorator covers `all_tags` for free: `ActionResolver::registry()` already returns `&R`, so when `R` becomes `Caching<GithubRegistry>` the loop at `src/upgrade/plan.rs:222` is cached with its text unchanged.

*Why over changing the port to `&mut self`:* that would remove the need for interior mutability, but contorts the port to serve one wrapper — viral up the call graph, blocks trait objects and shared ownership, makes cache-over-retry layering impossible under exclusive borrows, and forces a signature change onto two real implementations and nine fakes.

*Why over memoizing inside `ActionResolver`:* it already holds `&'reg R` and every command constructs one, so it looks like a cheaper home — no new dependency, no new directory. It fails on the same criterion as `ShaIndex`: `src/upgrade/plan.rs:163` and `:222` call `service.registry().all_tags(...)`, reaching *past* the resolver to the bare registry. A cache inside `ActionResolver` cannot see those calls, which are the ones that matter most.

*Why generic, not `Box<dyn VersionRegistry>`:* Rust decorators are conventionally generic. No allocation per layer, no dynamic dispatch, and the delegation inlines. The existing code is already generic over `R: VersionRegistry` everywhere, so nothing else changes shape.

*Scope note — `tags_for_sha` was dead and is now gone.* When this change was written the trait declared four methods, but `tags_for_sha` was only ever implemented, never called: `Registry::describe_sha` reached the same data through the *inherent* helper `get_tags_for_sha`, not the trait method. The decorator cached it anyway, for uniformity, while the user-facing requirement deliberately claimed nothing for it. Surfacing that dead method led to #180, and #141 removed it from the port; this change dropped its cache field and impl on rebase. The requirement never mentioned it, so the spec needed no edit — which is the payoff for not having advertised a benefit that could not materialize.

Interior mutability behind `&self` is the sanctioned use here. [`std::cell`](https://doc.rust-lang.org/std/cell/index.html) names it explicitly: "Introducing mutability 'inside' of something immutable... caching forces the implementation to perform mutation; or because you must employ mutation to implement a trait method that was originally defined to take `&self`." Cargo's own registry client takes the same shape — [`RemoteRegistry`](https://doc.rust-lang.org/nightly/nightly-rustc/cargo/sources/registry/remote/struct.RemoteRegistry.html) is `&self` plus `RefCell`/`OnceCell` because the `Source` trait's `query` takes `&self`.

### Decision 2: `elsa::FrozenMap` over `RefCell<HashMap>`

Four `FrozenMap` fields, one per method, keyed by that method's arguments.

The cache's contract is exactly `FrozenMap`'s contract: insert-only, never evicted, never overwritten, values live as long as the map. `FrozenMap::insert` and `get` both take `&self` and hand back `&V::Target`, so no `RefCell` borrow exists to be held across a call — the borrow-panic risk is eliminated by construction rather than by discipline.

*Dependency check against `deny.toml`:* `elsa` 1.11.2 is MIT/Apache-2.0, both on the `licenses.allow` list. Its only required dependency is `stable_deref_trait ^1.1.1`, which is **already in `Cargo.lock` at 1.2.1** (pulled through the gix tree), so the `bans.multiple-versions = "deny"` rule is not tripped and the effective new-crate count is one. `indexmap` is optional and stays off. No `skip` entry needed.

*Alternative considered — `RefCell<HashMap>`:* works, and is one fewer dependency. Rejected because it reinstates the objection the decorator is meant to dissolve: the natural `get_or_insert` shape wants to hold a borrow across the inner registry call that populates it, which panics at runtime rather than failing to compile. Avoiding that means splitting every method into "borrow, look up, drop the borrow, call, borrow again, insert" — five lines of discipline per method, four times over, with the failure mode being a panic in a user's terminal. `FrozenMap` costs one small dependency and makes the mistake unrepresentable. If `elsa` had been unwelcome under `deny.toml`, `RefCell` was the fallback and the discipline would have been stated as an invariant comment on each method; it is not needed.

*Alternative considered — the `cached` crate:* disqualified. Its macros cannot be applied to trait methods that take a receiver ("macro-defined functions cannot accept Self types as a parameter").

`FrozenMap` requires `V: StableDeref`, so values are boxed: `FrozenMap<Key, Box<Commit>>` and friends. `get`/`insert` return `&V::Target`, i.e. `&Commit`, which is cloned to satisfy the trait's by-value return. That clone is a few `String`s and always cheaper than an HTTP round trip.

### Decision 3: Cache successes only

A method that returns `Err` stores nothing, so a later call with the same key retries. This matters for the two recoverable errors the spec already classifies: `RateLimited` and `AuthRequired`. Caching a rate-limit failure would convert one transient failure into a run-wide failure for that key, which is strictly worse than the current behavior and invisible to the user. `ShaIndex` already behaves this way ("On error, nothing is stored") — the decorator preserves it.

The side effect is that a persistently failing key is retried once per call site rather than once per run. That is the same request count as today, so it is not a regression.

### Decision 4: `src/infra/registry/`, a new directory

The decorator is an adapter over a port, so it belongs in `src/infra/`, not `src/domain/`. Between the two candidate homes:

- `src/infra/github/` is at 4/8 files and would fit, but it is the *GitHub* adapter. The decorator is backend-agnostic — it wraps any `VersionRegistry`, and issue #145 adds a second backend it must also wrap. Filing it under `github/` would misname it.
- `src/infra/registry/` — chosen. `caching.rs` plus `mod.rs`, 2/8. #137's retry layer lands beside it as `retrying.rs`, which is the arrangement that makes the layer ordering legible at the composition root.

The 8-file budget counts `.rs` files per directory non-recursively. `src/infra/` holds 3 `.rs` files (`mod.rs`, `repo.rs`, `workflow_update.rs`), so adding a subdirectory does not press on it at all, and the new `src/infra/registry/` starts at 2.

### Decision 5: Delete `ShaIndex` outright

`get_or_describe` folds into `Caching::describe_sha`. `ShaIndex`, its `Default` impl, and the `sha_index: &mut ShaIndex` parameter come out of `ActionResolver::resolve_from_sha`, `lock_sync::update_lock`, `lock_sync::populate_lock_entry`, and `manifest_sync::upgrade_sha_versions_to_tags`. `manifest_sync` calls `resolver.registry().describe_sha(id, sha)` directly, which is now cached.

`tag_selection.rs` keeps `select_most_specific_tag` and `parse_version_components` and loses its `ShaIndex` import of `VersionRegistry`/`ShaDescription`. `src/domain/action/` stays at 8 files — no file is added there.

Keeping both mechanisms was rejected: two caches for one concern, with `describe_sha` double-cached and the reader left to work out which one is authoritative.

## Automated Test Strategy

**Level: unit, in isolation.** The decorator is tested against a purpose-built counting fake rather than through the commands, because "how many times did the network get hit" is not observable from a command's output — which is exactly the property under test.

- **New test infrastructure**: one counting fake registry, local to the decorator's test module, holding a `Cell<usize>` per method and returning canned successes. It is deliberately *not* added to `src/domain/resolution_testutil.rs` — keeping it local means the eight existing doubles stay untouched and unaware of caching, and issue #143's unified fake does not have to decide whether to model caching.
- **Critical path — one test per port method**: call the method twice with identical arguments, assert the returned values are equal and the fake's counter is 1. The set is exhaustive over the trait, so a method added to the port without a dedup test is a visible gap rather than a silent one.
- **Key discrimination — one test**: call a method twice with *different* arguments, assert the counter is 2. This is the test that would catch a key built from the wrong fields (e.g. `lookup_sha` keyed on `ActionId` alone, silently returning `v3`'s commit for `v4`). That failure mode is a wrong lock entry, not just a slow run, so it is the highest-value assertion here.
- **Error is not cached — one test**: a fake that always errors, called twice, must show a counter of 2.
- **Regression coverage for the deletion**: the existing `lock_sync`, `manifest_sync`, and `resolution` tests lose their `&mut sha_index` arguments but keep every assertion. They are the evidence that dropping the parameter changed no behavior — if `describe_sha` dedup had been load-bearing for correctness rather than efficiency, those tests fail.
- **Gate**: `mise run test` (typecheck, format, clippy, size budgets, lockfile, unit tests) and `mise run integ`. No new `#[ignore]`. No `tests/code_health.rs` budget is raised — the change is a net deletion and `src/infra/registry/` starts at 2 files.

## Observability

**How a cache failure would surface.** The decorator adds no new error variant and no new error path: on a miss it calls the inner registry and propagates its `Result` unchanged; on a hit it returns a clone of a value that already succeeded. Every error a user sees after this change is an error they would have seen before it, classified identically by the existing recoverable/strict rules.

**Can a failure be silent? Two ways, both addressed by design rather than by logging.**

1. *The cache returns a stale value* — impossible within a run: entries are never overwritten and never expire, and the map dies with the process, so nothing can be stale relative to when it was fetched. Across runs there is no cache at all.
2. *The cache returns the wrong value* — the real risk, and it would be silent: a mis-keyed entry produces a wrong SHA in `gx.lock` with no error, no warning, and no output difference. Nothing at runtime can detect it, which is why the key-discrimination test above exists. The mitigation is compile-time and test-time, not observational: each key is a tuple of exactly the method's own arguments, and a test asserts that distinct arguments do not collide.

**No new instrumentation.** There is deliberately no hit/miss counter, no progress line, no debug output. `gx` has no logging framework and no verbosity flag, and the user-visible signal for this change is the *absence* of rate-limit warnings during a run — which the existing `RecoverableWarning` event already reports. Adding a cache-statistics surface would be speculative: it is config surface the proposal rules out, and nobody has asked to see it. If a future bug report needs the numbers, the counting fake in the test module is the tool that answers it.

## Risks / Trade-offs

- **A wrong cache key silently corrupts `gx.lock`** → Keys are tuples of exactly the arguments each method receives, and a dedicated test asserts distinct arguments produce distinct lookups. This is the only failure mode that changes user-visible output, so it gets the direct test rather than being folded into the dedup tests.
- **Unbounded memory growth** → Bounded in practice by the number of distinct actions in one repository's workflows — tens, not thousands — times a handful of `String`s each. A run that could exhaust memory would have exhausted the API rate limit first by orders of magnitude. An eviction policy would be speculative complexity.
- **One new direct dependency for a data structure the standard library nearly covers** → Accepted. Verified against `deny.toml`: allowed license, no version conflict, its single required dependency already present in the lock. The alternative trades one crate for a runtime panic hazard repeated across four methods.
- **A long-running process would never see new tags** → Not applicable: `gx` is a CLI that exits after one command. The spec states the per-run scope so the constraint is recorded rather than assumed.
- **Refactor blast radius across ~28 sites** → Nearly all mechanical parameter removal in tests, and the compiler finds every one. This is the acknowledged cost of the decision; it is paid once and is a net deletion.
- **Concurrent work in adjacent files** → `src/infra/github/resolve.rs` is being split by other work and is not touched here. The edit to `src/domain/resolution.rs` is confined to removing one parameter and its import.

## Migration Plan

No migration. No user-visible interface, file format, or configuration changes — `gx.toml` and `gx.lock` are byte-identical for identical inputs. Rollback is a revert of the commit.

## Open Questions

None. The dependency question (`elsa` vs `RefCell`) was the one open item and is resolved above against `deny.toml`.
