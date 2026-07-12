## Why

`gx tidy` can write a `gx.lock` entry whose resolved version violates the manifest's declared range and report success — e.g. manifest `"^5"` with a workflow SHA whose tag is `v6.0.2` produces `"^5" -> v6.0.2` ([#95](https://github.com/gmeligio/gx/issues/95)). The root cause is a domain-model gap: tidy's SHA-first path treats a workflow's pinned SHA as authoritative over the *version label*, never consulting the manifest range. The invariant that binds them — a lock version must satisfy its specifier — is stated nowhere, so no code path enforces it. This passes the relevance gate (it changes user-facing `gx tidy` behavior and states a new domain invariant, not just an internal refactor).

## What Changes

- State the invariant **`lock.version` must satisfy its `specifier`** in the `manifest-and-lock` spec. The manifest range is authoritative; the pinned SHA and locked version are a *preference* (the model pnpm, uv, and Cargo converge on).
- `gx tidy`: when a workflow's pinned SHA carries a tag that falls **outside** the manifest range, treat the pin as a stale preference — keep the SHA authoritative for *which commit*, but re-resolve the *version label* within the declared range and repin the workflow accordingly. No silent out-of-range lock entry is ever written.
- Add `Specifier::matches_version(&Version) -> bool` as the single satisfiability primitive. Non-semver specifiers (`main`, bare SHA) are **exempt** (never flagged as violations).
- `gx init` inherits the shared code path but is unaffected: it only ever hits the new-action/derive branch where the specifier is manufactured from the SHA's tag, so the invariant holds by construction. A guard test locks this in.
- `gx upgrade` already implements this model (`find_upgrade_candidate`'s `InRange`/`CrossRange`, manifest-authoritative) and has **no behavior change**; a no-regression test locks it in.
- **Out of scope (follow-up):** a `--frozen`/`--locked` check mode that errors instead of mutating when the lock is out of sync. This change makes tidy self-heal; the assertion mode is a separate capability.

## Capabilities

### New Capabilities
_(none — this states an invariant and reconciliation rule on the existing manifest/lock capability rather than introducing a new user-facing concept)_

### Modified Capabilities
- `manifest-and-lock`: add the requirement that a resolved lock version must satisfy the specifier it is keyed under, and that tidy reconciles an out-of-range pinned SHA by re-resolving within the manifest range rather than recording the out-of-range tag.

## Impact

- **Spec**: `openspec/specs/manifest-and-lock/spec.md` — new requirement + scenarios.
- **Domain**: `src/domain/action/specifier.rs` — add `matches_version`; expose the `Version`→`semver` bridge (currently `pub(super) parse_semver`).
- **Tidy**: `src/tidy/lock_sync.rs` (and possibly `src/domain/resolution.rs`) — gate the SHA-first path on the manifest range; re-resolve within range on violation. Shared by `gx init`.
- **Tests**: #95 regression (`^5` + `v6` SHA), sub-major variant (`~1.15.2` + `v1.16.0`), `@main` exemption, init derive-branch guard, upgrade no-regression.
- No file-format change; no breaking change to `gx.toml`/`gx.lock`.
