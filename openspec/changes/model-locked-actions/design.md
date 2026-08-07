## Context

`Lock` is `HashMap<Spec, LockEntry>` (`src/domain/lock.rs:32`). `LockEntry { reference, commit }` (`:13`) does not carry its own `Spec` — the key lives outside the value, so no single value in the domain represents "one managed dependency as recorded in the lock".

Current access splits cleanly in two:

- **Point lookups** — `Lock::get`/`has`, used by `src/lint/stale_comment.rs:27`, `src/lint/sha_mismatch.rs:25`, `src/upgrade/plan.rs:226`, `src/tidy/patches.rs:60`, `src/tidy/lock_sync.rs`. The caller already holds the `Spec`, so it needs nothing more than the value.
- **Whole-collection iteration** — `Lock::entries()` (`:94`), with exactly one production consumer: `build_lock_document` in `src/infra/lock/format.rs:115`. It immediately re-pairs each key with its value to sort, and again at `:153` to deduplicate commits.

`gx audit` (#129–#133) adds three or four more whole-collection consumers, each needing the same tuple: `ActionId`, `Specifier`, `ResolvedRef`, `CommitSha`, `Repository`.

Binding constraints:
- `gx.lock` must round-trip byte-identically. No format change.
- `src/domain/` is at 6/8 `.rs` files; `src/domain/action/` is at 8/8 (full).
- `tests/code_health.rs` budgets may not be raised. `src/domain/lock.rs` is 433 total / 550 budget.

## Goals / Non-Goals

**Goals:**
- Name the lock's row concept and give it a self-contained type.
- Make `Lock` iterable as a collection of managed dependencies, so a consumer never reassembles key-onto-value.
- Serve the four known audit consumers with exactly the fields they need.

**Non-Goals:**
- Changing the `gx.lock` file format in any way.
- Writing any audit check. This is the type, not its users.
- Removing `Commit.repository` (see Decisions).
- Reworking `Lock::get`, `set`, `set_version`, `retain`, `diff`, or `is_complete`.
- Adding a field or accessor no known consumer needs.

## Decisions

### Introduce `LockedAction` as a borrowed view, not an owned replacement

`LockedAction<'lock>` holds `spec: &'lock Spec` plus `entry: &'lock LockEntry`, and exposes `id()`, `specifier()`, `sha()`, `repository()`, `commit()`, `version_label()`.

*Why borrow the whole `LockEntry` rather than its two fields separately:* the row's value is already one struct, so splitting it into `reference` and `commit` references duplicates `LockEntry`'s shape in a second place that must be kept in step with it. Holding the entry lets `version_label()` delegate to `LockEntry::version_label()` — the same expression `format.rs` wrote before this change — instead of reimplementing the label rule.

*No `reference()` accessor:* per the Non-Goal above, an accessor no known consumer needs is not added. `build_lock_document` reads the reference only through `version_label()`, and the audit checks in #129–#133 need the label too, not the `ResolvedRef`. `commit()` exists because `format.rs`'s dedup map stores `&Commit` directly.

*Why borrowed:* `entries()` is called on a `&Lock` in the hot serialization path and, per #129–#133, on every audit run. An owned type would clone a `Spec` and `Commit` per row for read-only consumers. Borrowing costs nothing and keeps `Lock` the single owner.

*Why keep `LockEntry`:* it is the map's storage value and the shape `Lock::set` inserts, `set_version` mutates in place, and `LockDiff::added` owns (`src/domain/diff.rs:48`). Replacing it would ripple into `diff.rs` and every `LockDiff` consumer for no gain. `LockEntry` is storage; `LockedAction` is the read view over a row. The two coexist deliberately.

*Alternative rejected — put `Spec` inside `LockEntry` and make `Lock` a `Vec<LockEntry>`:* duplicates the key inside the value (two sources of truth that can disagree), and turns every point lookup — the majority of call sites — from O(1) into a scan.

*Alternative rejected — `entries()` yields `(&Spec, &LockEntry)` and audit builds its own row struct:* that is precisely the four-way reinvention this change exists to prevent.

### Name: `LockedAction`

The domain already says `ResolvedAction` for "what goes into the workflow file" (`src/domain/action/resolved.rs:102`). `LockedAction` reads in the same register: an action, as locked. It says *locked* rather than *lock entry* because the concept is the dependency, not the map slot — `LockEntry` remains the map slot's name, and the two must not blur.

### File: new `src/domain/locked_action.rs`

`src/domain/action/` is full at 8/8, so the type cannot live beside `resolved.rs`. `src/domain/` has 2 free slots; this consumes one, leaving one. Keeping it out of `lock.rs` also keeps that file's budget headroom for its existing tests.

### `Commit.repository` is retained

It is genuinely derived — always `id.base_repo()` at write time (`src/infra/github/registry.rs:147,225`) — and its only production reads are the emptiness check in `is_complete` (`src/domain/lock.rs:72`) and re-serialization (`src/infra/lock/format.rs:211`). Dropping the struct field is nonetheless out of scope: `repository` is a persisted key in the lock's `[actions]` tier, written at `format.rs:211` and read back at `:87`. Removing it either deletes a key from every `gx.lock` on disk or forces re-derivation at load — both are format changes this change forbids. `LockedAction::repository()` therefore surfaces the stored value, which is what an audit check comparing lock against reality should see. Revisit under a deliberate lock-format revision.

### `Lock::entries()` changes signature rather than gaining a sibling

Returning `impl Iterator<Item = LockedAction<'_>>` and updating the single production consumer is smaller than keeping two iterators alive and having to decide, at each future call site, which one to reach for. One way to iterate the lock.

The blast radius is exactly one call site. A repo-wide search for `entries()` across `src/` and `tests/` returns four hits, three of which are test *function names* (`plan_one_new_action_produces_added_entries`, `plan_removed_action_produces_removed_entries`, `e2e_upgrade_preserves_unaffected_entries`) rather than calls. The only true caller is `src/infra/lock/format.rs:115`. There are no test consumers to migrate, so the signature change is fully bounded.

## Automated Test Strategy

Unit level, in-crate — this is a domain type with no I/O.

- **Critical path — format stability.** A test in `src/infra/lock/tests.rs` asserts that parsing a fixed `gx.lock` string and re-serializing it yields the byte-identical input. This is the load-bearing assertion of the whole change: it fails if `LockedAction` perturbs field order, sort order, or dedup behavior in `build_lock_document`. It must be written against a literal expected string, not a round-trip through the same code.
- `src/domain/locked_action.rs` unit tests: a `LockedAction` exposes the same `id`/`specifier`/`sha`/`repository`/`version_label` as the row it views, including the bare-commit case where `version_label()` is the SHA (`ResolvedRef::label`).
- `src/domain/lock.rs`: `entries()` yields one `LockedAction` per stored row, each carrying its own spec.
- Existing coverage is the regression net: `src/infra/lock/format.rs` round-trip and sort-order tests, `src/infra/lock/tests.rs`, `tests/integ_*.rs`. Gate is `mise run test`, plus `mise run integ`.

No new test infrastructure.

## Observability

No new runtime failure modes. `LockedAction` is a borrowed view with no fallible construction — it performs no parsing, no I/O, and no allocation, so constructing one cannot fail.

**Construction cannot fail, but a row can still be incomplete.** `Lock::is_complete` exists because the loaded shape permits rows whose fields are empty, and this change deliberately leaves it unreworked. `entries()` therefore yields a `LockedAction` for incomplete rows too. Where absence is representable, the accessor makes it unignorable rather than surfacing a usable-looking empty value: `repository()` returns `Option<&Repository>`, `None` for a row that never stored one. That is the accessor `gx audit` hits first — a check building `GET /repos/{owner}/{repo}` from an empty string would request a malformed URL and report clean, so the type forces the caller to handle absence. Accessors that cannot be absent (`id()`, `specifier()`, `sha()`) stay plain references. Completeness across all five fields remains `Lock::is_complete`'s question, unreworked here. Recording this here is the point of the change: the zip is now modeled, so the completeness caveat should not be rediscovered four times.

The one behavior that could regress *silently* is lock-file serialization: a reordering or dedup bug in `build_lock_document` would produce a valid-but-different `gx.lock` that no error path would surface, showing up only as spurious churn in a user's diff. That is exactly what the byte-identity test guards, and it is why that test asserts against a literal string rather than a self-round-trip.

Error classification is unchanged: no diagnostic, warning, or exit code moves.

## Risks / Trade-offs

- **Silent lock churn** — a subtle change in `build_lock_document`'s sort or dedup emits a valid but differently-ordered `gx.lock`, dirtying user diffs with no error. → Byte-identity test against a literal fixture, plus the existing sort-order test at `format.rs:302`.
- **Two row-ish types (`LockEntry`, `LockedAction`) invite confusion** — a future contributor may not know which to reach for. → Doc comments state the split explicitly: `LockEntry` is what the map stores and mutates; `LockedAction` is what iteration yields. Only `LockedAction` is reachable from `entries()`, so the iteration path has no choice to get wrong.
- **The lifetime parameter leaks into audit signatures** — `fn check(action: LockedAction<'_>)` rather than an owned parameter. → Accepted; it is a standard Rust borrowed-view signature, and cloning per row on every audit run to avoid a `'_` is the worse trade.
- **`src/domain/` drops to 7/8 files** — one slot left before the directory budget bites. → Noted for whoever needs the last slot; the alternative (`src/domain/action/`) is already full, so there is no cheaper placement.
