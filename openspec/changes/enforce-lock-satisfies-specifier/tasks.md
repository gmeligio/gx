## 1. Failing regression tests (must fail on current main)

- [x] 1.1 Add a `lock_sync` test: manifest `^5`, workflow SHA tagged `v6.0.2`, registry offers `v5.4.0`; assert the lock version satisfies `^5` and is never `v6.0.2`. Confirm it FAILS against current code.
- [x] 1.2 Add the sub-major variant test: manifest `~1.15.2`, workflow SHA tagged `v1.16.0`, registry offers `v1.15.9`; assert resolution satisfies `~1.15.2`. Confirm it FAILS.

## 2. Satisfiability primitive

- [x] 2.1 Add `Specifier::matches_version(&self, version: &Version) -> bool` in `src/domain/action/specifier.rs`: parse the version via the canonical `parse_semver`, delegate to `matches`; non-semver specifiers (`Ref`/`Sha`) return exempt (accept any version).
- [x] 2.2 Unit-test `matches_version`: `^5`+`v6.0.2`→false, `^5`+`v5.4.0`→true, `~1.15.2`+`v1.16.0`→false, `~1.15.2`+`v1.15.9`→true, `main`→exempt, bare-SHA specifier→exempt.

## 3. Tidy reconciliation (SHA-first path)

- [ ] 3.1 In `src/tidy/lock_sync.rs::populate_lock_entry`, after the SHA-first `resolve_from_sha` returns, gate on the manifest range: when the specifier is a semver range (`precision().is_some()`) and `matches_version(resolved.version)` is false, fall back to the version-first `resolver.resolve(spec)` so the version is re-resolved within the range. Keep `resolve_from_sha` signature unchanged.
- [ ] 3.2 Confirm tasks 1.1 and 1.2 now PASS.
- [ ] 3.3 Verify the workflow is repinned to the in-range resolution (no `v6.0.2` written under the `^5` key); add/extend a test asserting the resulting lock entry and workflow patch.

## 4. Observability

- [ ] 4.1 Emit a progress event when an out-of-range pin is re-resolved, naming the action, the rejected tag, and the in-range version chosen (mirror `SyncEvent::VersionCorrected`/`ShaUpgraded`, flow through `on_progress`).
- [ ] 4.2 Test that the event fires on the #95 scenario and does not fire on the in-range and non-semver scenarios.

## 5. Guard the unaffected paths

- [ ] 5.1 Add an init guard test: empty manifest + workflow `@sha # v6` (tag `v6.0.2`) → derived `^6`, lock `v6.0.2`, no re-resolution/violation triggered.
- [ ] 5.2 Confirm all existing `src/upgrade/plan.rs` tests remain green unchanged (upgrade behavior is byte-identical).

## 6. Docs and verification

- [ ] 6.1 Update `README.md` and/or `docs/demo.tape` only if user-facing tidy output/behavior described there changed (per AGENTS.md).
- [ ] 6.2 Run `mise run test-all` and `mise run clippy`; ensure clean.
- [ ] 6.3 Manually verify against the #95 repro (manifest `^5`, workflow pinning a `v6` SHA): `gx tidy` produces an in-range lock and prints the re-resolution notice.
