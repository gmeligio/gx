## 1. Failing regression tests (must fail on current main)

- [x] 1.1 Add a `lock_sync` test: manifest `^5`, workflow SHA tagged `v6.0.2`, registry offers `v5.4.0`; assert the lock version satisfies `^5` and is never `v6.0.2`. Confirm it FAILS against current code.
- [x] 1.2 Add the sub-major variant test: manifest `~1.15.2`, workflow SHA tagged `v1.16.0`, registry offers `v1.15.9`; assert resolution satisfies `~1.15.2`. Confirm it FAILS.

## 2. Satisfiability primitive

- [x] 2.1 Add `Specifier::matches_version(&self, version: &Version) -> bool` in `src/domain/action/specifier.rs`: parse the version via the canonical `parse_semver`, delegate to `matches`; non-semver specifiers (`Ref`/`Sha`) return exempt (accept any version).
- [x] 2.2 Unit-test `matches_version`: `^5`+`v6.0.2`→false, `^5`+`v5.4.0`→true, `~1.15.2`+`v1.16.0`→false, `~1.15.2`+`v1.15.9`→true, `main`→exempt, bare-SHA specifier→exempt.

## 3. Tidy reconciliation (SHA-first path)

- [x] 3.1 In `src/tidy/lock_sync.rs::populate_lock_entry`, after the SHA-first `resolve_from_sha` returns, gate on the manifest range: when the resolution is tag-backed (`ref_type != Commit`) and `matches_version(resolved.version)` is false, fall back to the version-first `resolver.resolve(spec)` so the version is re-resolved within the range. Keep `resolve_from_sha` signature unchanged. (A SHA with no tags resolves to the bare commit and carries no version label to constrain — exempted.)
- [x] 3.2 Confirm tasks 1.1 and 1.2 now PASS.
- [x] 3.3 Verify the workflow is repinned to the in-range resolution (no `v6.0.2` written under the `^5` key); asserted by the regression tests' `assert_ne!`/`assert_eq!` on the resulting lock version.

## 4. Observability

- [x] 4.1 Emit a progress event when an out-of-range pin is re-resolved, naming the action, the rejected tag, and the in-range version chosen (mirror `SyncEvent::VersionCorrected`/`ShaUpgraded`, flow through `on_progress`).
- [x] 4.2 Test that the event fires on the #95 scenario and does not fire on the in-range and non-semver scenarios.

## 5. Guard the unaffected paths

- [x] 5.1 Add an init guard test: empty manifest + workflow `@sha # v6` (tag `v6.0.2`) → derived `^6`, lock `v6.0.2`, no re-resolution/violation triggered. (`init_derived_specifier_keeps_sha_version`)
- [x] 5.2 Confirm all existing `src/upgrade/plan.rs` tests remain green unchanged (upgrade behavior is byte-identical; `src/upgrade/` and `upgrade.rs` untouched per git).

## 6. Docs and verification

- [x] 6.1 Update `README.md` and/or `docs/demo.tape` only if user-facing tidy output/behavior described there changed (per AGENTS.md). — No change needed: this is a correctness fix; README/demo describe commands, not internal range-reconciliation semantics.
- [x] 6.2 Run `mise run test:all` and `mise run clippy:check`; ensure clean. — 408 unit + all integration + 15 e2e green; clippy:check clean.
- [x] 6.3 Manually verify against the #95 repro (manifest `^3`, workflow pinning a real `v4` SHA): `gx tidy` produced an in-range `^3 → v3` lock and printed `pinned v4.3.1 is outside the range; re-resolved to v3`.
