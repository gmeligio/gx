## Context

gx separates three concerns (from the `semver-specifiers` design): a **Specifier** (a range like `^5`, the user's declared intent, living in the manifest and lock key), a **Version** (a resolved tag like `v6.0.2`, in the lock entry), and a **SHA** (the exact commit, in the lock entry and workflow). Design law binds them: `Specifier.matches(Version)` must hold.

Three resolution paths exist, and only one enforces that law:
- `upgrade.rs::find_upgrade_candidate` — range-aware, produces `InRange`/`CrossRange`; `CrossRange` rewrites the specifier before resolving, so the invariant holds by construction.
- `resolution.rs::resolve` (version-first) — derives its lookup tag *from* the specifier, so it physically cannot overshoot.
- `resolution.rs::resolve_from_sha` (SHA-first) — takes `(id, sha)`; the `Specifier` is not in its signature. It derives the version from whatever tag the pinned SHA carries, with no range awareness. **This is where #95 lives.**

`gx tidy` (and `gx init`, which calls `tidy::plan`) uses the SHA-first path at `lock_sync.rs:97`. When a tracked action already has a declared range and a workflow SHA whose tag exceeds it, tidy writes the out-of-range version and reports success.

Research into pnpm, uv, and Cargo (all three, unanimously): the declared constraint is authoritative; the locked/pinned identity is a **preference** discarded the moment it violates the constraint. gx's SHA-first path is missing exactly that discard-and-re-resolve step.

## Goals / Non-Goals

**Goals:**
- State the `lock.version ⊨ specifier` invariant in the spec so every write path is bound by it.
- Make tidy's SHA-first path range-aware: an out-of-range pinned SHA's *commit authority* is kept, but its *version label* is re-resolved within the manifest range.
- Introduce one shared satisfiability primitive (`Specifier::matches_version`) that tidy uses now and any future lint/frozen check can reuse.
- Preserve `gx init`'s correct SHA-authority-over-version behavior (fresh adoption derives the specifier from the SHA, so it never conflicts).
- Leave `gx upgrade` behaviorally unchanged (it already implements this model).

**Non-Goals:**
- A `--frozen`/`--locked` assertion mode that errors instead of mutating. Tidy self-heals here; the assertion mode is a separate capability (follow-up).
- Changing the file format of `gx.toml` or `gx.lock`.
- Collapsing the two `parse_semver` copies (`specifier.rs` and `upgrade.rs`) — a nice cleanup, but out of scope; flagged for later.
- Changing how init/tidy discover *new* actions or derive their initial specifier.

## Decisions

### D1: The satisfiability primitive is `Specifier::matches_version`, not `find_upgrade_candidate`

Tidy validating a single pinned SHA's tag against a range is a *boolean* question. `find_upgrade_candidate` answers a different, richer question — "given a menu of all tags, which is the best *higher* one, and does picking it move the range?" — with floor logic, pre-release filtering, and `max_by` selection that are upgrade-only and would corrupt tidy's answer (e.g. it discards any tag `≤ floor`, but a pin below the floor is fine for tidy).

The actual satisfiability core in `find_upgrade_candidate` is already one line: `specifier.matches(&best_semver)` (`upgrade.rs:178`). `Specifier::matches` (`specifier.rs:72`) wraps `semver::VersionReq::matches` and already returns `false` for `Ref`/`Sha`.

**Decision:** add `Specifier::matches_version(&self, version: &Version) -> bool` on `Specifier` that does `parse_semver(version) + matches` internally. Tidy calls one method; the `Version`→`semver` bridge lives in one place.

**Alternative considered — route tidy through `find_upgrade_candidate`:** rejected. Category error (search vs. validate); would mis-handle pins below the floor.

**Alternative considered — bump `parse_semver` to `pub(crate)` and inline in tidy:** rejected. Leaks a helper across module trees and re-parses in a second place. The method wrapper keeps the invariant a first-class, single-call domain concept.

### D2: Exempt vs. violating — guard on the range, not on bare `!matches`

`matches()` returns `false` for non-semver specifiers (`main`, bare SHA). A naive "if `!matches` then re-resolve" would wrongly flag every `@main`. The check must fire only when the specifier is a semver range.

**Decision:** the reconciliation only applies when `specifier.precision().is_some()` (i.e. it is a `Range`). Otherwise the pin is accepted as-is. `matches_version` may encode this as a tri-state at the call site, but the load-bearing rule is: *non-semver = exempt, semver range + out-of-range tag = re-resolve.* This mirrors `find_upgrade_candidate`'s `precision()?` early-return.

### D3: Gate at the tidy call site, keep `resolve_from_sha` pure

Two placements were considered:
- **(a)** thread `&Specifier` into `resolve_from_sha` and gate inside `resolution.rs`.
- **(b)** keep `resolve_from_sha` specifier-agnostic; in `populate_lock_entry` (`lock_sync.rs`), after the SHA-first resolve returns, check `specifier.matches_version(resolved.version)` and, on a range violation, fall back to the version-first `resolver.resolve(spec)` (which resolves within range).

**Decision: (b).** It keeps `resolution.rs` a pure metadata deriver, confines the policy to tidy's reconciliation layer where it belongs, and reuses the *already existing* `.or_else(|_| resolver.resolve(spec))` fallback structure at `lock_sync.rs:97-98` — the violation just becomes another reason to take that fallback. Smaller blast radius, no signature churn on the shared resolver.

### D4: Init is safe by construction — via the new-vs-existing seam, not a special case

`gx init` runs on an empty manifest, so it only ever hits the *new-action* branch of `manifest_sync`, where the specifier is derived from the SHA's own tag. `matches_version` therefore holds trivially — the tag *is* the range's origin. The reconciliation in D3 only triggers for a *pre-declared* specifier that the SHA's tag violates, which init cannot produce. No init-specific code; a guard test proves the derive-branch does not trip the check.

### D5: Tidy re-resolves (self-heals); it does not block

The earlier journey analysis suggested tidy *block* on a cross-range pin. The pnpm/uv/Cargo model corrects this: the *mutating* command (tidy ≈ `pnpm install`) re-resolves within the constraint; *blocking* belongs to a check mode (`--frozen`), which is out of scope here. Re-resolving is better UX for the common "bumped the workflow, forgot the manifest" case — it lands the consistent state instead of wedging the commit. If the user genuinely wanted the newer major, the honest path is `gx upgrade`, which moves the declared intent deliberately.

## Automated Test Strategy

Unit/domain tests are the critical path — the bug is a pure domain reconciliation gap with no I/O. Per `AGENTS.md`, the bug-fix test must FAIL against current `main` (asserting correct behavior), not document the broken behavior.

- **`Specifier::matches_version` unit tests** (`specifier.rs`): `^5`+`v6.0.2`→false, `^5`+`v5.4.0`→true, `~1.15.2`+`v1.16.0`→false, `~1.15.2`+`v1.15.9`→true, `main`+anything→true (exempt), bare-SHA specifier→exempt.
- **Tidy reconciliation tests** (`lock_sync.rs`, using `FakeRegistry` with `with_sha_tags`): the #95 regression — manifest `^5`, workflow SHA tagged `v6.0.2`, registry offers `v5.4.0`; assert the lock version satisfies `^5` and is never `v6.0.2`. Plus the sub-major `~1.15.2`/`v1.16.0` variant. These are the tests that must fail on `main` first.
- **Init guard test** (`init` or `lock_sync`): empty manifest + `@sha # v6` (tag `v6.0.2`) → derived `^6`, lock `v6.0.2`, no violation/fallback triggered.
- **Upgrade no-regression**: existing `upgrade/plan.rs` tests must stay byte-identical green; add nothing unless a shared-primitive refactor touches them.
- **e2e** (`tests/e2e_github.rs`, gated on `GITHUB_TOKEN`): optional — a real action with a moved major tag exercises the full path; only if a cheap real-world fixture exists.

Success criteria: all existing tests green; new tidy regression + sub-major tests fail on `main` and pass after; `mise run test-all` clean.

## Observability

The failure this change targets is currently *silent* — that is the defect. Surfacing is therefore part of the fix:

- When tidy re-resolves an out-of-range pin, emit a progress/`SyncEvent` line naming the action, the rejected out-of-range tag, and the in-range version chosen (e.g. `actions/checkout: pinned v6.0.2 is outside ^5; re-resolved to v5.4.0`). This mirrors the existing `SyncEvent::VersionCorrected`/`ShaUpgraded` events and flows through the same `on_progress` callback the pre-commit hook and CI verbose mode already print.
- No failure is silent: if the fallback re-resolution itself fails (registry error), it surfaces via the existing recoverable/strict error classification (`ResolutionSkipped` warning or hard `ResolutionFailed`), unchanged.
- `gx lint` continues to see the reconciled (now in-range) lock, so no diagnostic is needed there for the fixed case; a lint rule remains available as a future check-mode surface if desired.

## Risks / Trade-offs

- **[Re-resolution changes the pinned commit the user had in the workflow]** → This is intended (the pin was inconsistent with declared intent), but it must be *visible*. Mitigation: the D-observability progress line + the workflow diff tidy already produces; the user sees exactly what changed and can `gx upgrade` if they meant the newer major.
- **[Registry offers no in-range tag]** (e.g. `^5` but the action only ever published `v6`) → the version-first fallback resolves the range's lookup tag (`v5`) and fails if absent, surfacing as a normal resolution error rather than a silent bad lock. Acceptable and honest; documented via the existing error path.
- **[Two `parse_semver` copies could drift]** → not introduced by this change; `matches_version` uses the canonical `specifier.rs` one. Flagged as follow-up cleanup, not blocking.
- **[Scope creep toward a frozen mode]** → explicitly deferred; this change only makes tidy self-heal, keeping the diff small and reviewable.
