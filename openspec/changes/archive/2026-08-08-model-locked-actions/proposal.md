## Why

The lock's rows have no name. `Lock` stores `HashMap<Spec, LockEntry>` where the key lives outside the value, so a row is only a complete thought when a caller zips `&Spec` back onto `&LockEntry` by hand. The upcoming `gx audit` command (#129–#133) iterates `gx.lock` and every one of its four checks needs the same per-row tuple — action id, specifier, resolved reference, commit SHA, repository. Without a modeled type, four sub-issues each reinvent the zip.

The same gap already shows up today: `Lock::is_complete` (`src/domain/lock.rs:64`) is a runtime five-field emptiness check that exists because the loaded shape permits rows that aren't whole, and the sole production consumer of `Lock::entries()` — the TOML serializer at `src/infra/lock/format.rs:115` — spends its first 30 lines re-pairing keys with values before it can sort or write them.

## What Changes

- Introduce `LockedAction` in `src/domain/locked_action.rs`: a self-contained, borrowed view of one lock row carrying its `Spec` alongside the `LockEntry` it maps to, with accessors for the fields audit needs (`id`, `specifier`, `sha`, `repository`, `commit`, `version_label`). `repository()` returns `Option` so a row that stored none cannot be mistaken for a usable value.
- Change `Lock::entries()` to yield `LockedAction` instead of `(&Spec, &LockEntry)`, so the lock reads as a collection of managed dependencies rather than a map that callers must reassemble.
- Update the one production consumer (`src/infra/lock/format.rs`) to iterate `LockedAction`.
- Keep `LockEntry` as the map's storage value. It stays the in-place-mutable shape `Lock::set`/`set_version` need; `LockedAction` is the read view over a row.

Not in scope, deliberately:
- **No lock file format change.** `gx.lock` must round-trip byte-identically; a test asserts it.
- **No new audit logic.** This models the row the four known consumers need and nothing beyond it.
- `Commit.repository` is **retained** — see Impact.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None.

**Spec relevance gate — skipping specs.** This matches the config's *"Skip spec: internal refactoring with no user-visible change"* rule. `LockedAction` is a private domain type behind the `Lock` API; no command, flag, output line, error message, or file format changes. It fails both requires-spec gates: it neither adds/removes/changes user-facing behavior, nor introduces a domain concept that changes what users can do — `gx audit` will change what users can do, but that is #129–#133's spec to write, not this prerequisite's. A spec here would have no user to name and nothing they'd notice, violating *"Every spec must trace to something a user cares about."*

## Impact

- `src/domain/lock.rs` — `entries()` return type; `LockEntry` retained as storage.
- `src/domain/locked_action.rs` — new file. `src/domain/` is at 6/8 files, leaving one slot after this. `src/domain/action/` is at 8/8 and must not be touched.
- `src/infra/lock/format.rs` — the sole production consumer of `entries()`, updated to the new iterator item.
- Point-lookup callers (`src/lint/stale_comment.rs:27`, `src/lint/sha_mismatch.rs:25`, `src/upgrade/plan.rs:226`, `src/tidy/**`) go through `Lock::get`/`has` and are **unaffected**.
- **`Commit.repository` is kept.** It is persisted in the lock's `[actions]` tier (`src/infra/lock/format.rs:211`, read back at `:87`). Although it is always `id.base_repo()` at write time (`src/infra/github/registry.rs:147,225`), dropping the field would either delete a key from every `gx.lock` on disk or force re-derivation at load — a lock-format change, which this change forbids. It is also one of the five fields `is_complete` validates. Out of scope; noted for a future format revision.
- No user-facing surface changes, so no README or `docs/demo.tape` update is needed.
