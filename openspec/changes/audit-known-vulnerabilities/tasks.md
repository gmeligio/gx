## 1. Reshape the advisory seam to wholesale

- [ ] 1.1 Add `package: String` to `Advisory` in `src/infra/github/advisory.rs`, populated from `package { name }`.
- [ ] 1.2 Replace `ADVISORY_QUERY` with the unparameterized ecosystem query selecting `totalCount` and `package { name }`; drop the `$package` variable and the `Variables` struct.
- [ ] 1.3 Change the `AdvisoryQuery` trait method to `all_actions_advisories(&self) -> Result<Vec<Advisory>, Error>`; update `GraphQlAdvisories`.
- [ ] 1.4 In `interpret`, error when `totalCount` exceeds the returned node count, so a truncated page can never read as a complete clean result.
- [ ] 1.5 Update `FakeAdvisories` to the new method, keeping `new(...)`, `failing()`, and a call counter proving exactly one query per run.
- [ ] 1.6 Update the existing advisory unit tests to the new shape; add one asserting the truncation guard errors.

## 2. Re-add `repository` to the audit target

- [ ] 2.1 Add the `repository: &'lock Repository` field to `AuditTarget` in `src/audit/target.rs` and the one line in `targets()` that populates it from the lock row.
- [ ] 2.2 Confirm no other check's file is touched.

## 3. Register the check name

- [ ] 3.1 Add `KnownVulnerability => "known-vulnerability",` as a single line inside the `rule_ids!` invocation in `src/audit/check_name.rs`. No hand-written `Display`/`FromStr`/serde.

## 4. Implement the check

- [ ] 4.1 Create `src/audit/advisory_check.rs` (one of the four remaining `src/audit/` slots).
- [ ] 4.2 Implement local range matching returning a three-way outcome — affected, unaffected, undetermined — using `semver::VersionReq` for the range and `semver::Version` for the locked version.
- [ ] 4.3 Normalize the locked version before parsing: strip a leading `v`/`V`, zero-pad partial versions (`41` → `41.0.0`, `4.2` → `4.2.0`), following the convention `Version::precision()` already sets.
- [ ] 4.4 Index advisories once into a map keyed on the lowercased package name; look up by the target's `repository`, case-insensitively.
- [ ] 4.5 Emit an error-level finding naming action, locked version, GHSA id, severity, affected range, permalink, and first patched version when present.
- [ ] 4.6 Emit a `warn`-level undetermined finding when an action has advisories but the version or range could not be interpreted.
- [ ] 4.7 Register the check with one line in `collect_findings` in `src/audit/mod.rs`; thread `&dyn AdvisoryQuery` through its signature.
- [ ] 4.8 Add the `Error` variant for a failed lookup in `src/audit/mod.rs`; construct `GraphQlAdvisories` in `Audit::run` after the existing token guard.

## 5. Test

- [ ] 5.1 Table-test range matching over every boundary in the design's table, asserting both sides of each: `< 46.0.1`, `>= 2.25.0, < 2.37.1`, `<= 0.24.0`, `<= 45.0.7`, `< 41`, `>= 87, < 90`.
- [ ] 5.2 Assert the `v` prefix in both directions, including that `45.0.7` and `v45.0.7` reach the same verdict.
- [ ] 5.3 Test check behavior via `FakeAdvisories`: affected yields one error finding carrying every required element; unaffected yields nothing; an advisory for an unlocked package yields nothing; a mixed-case package name still matches.
- [ ] 5.4 Test that a branch-pinned action with advisories yields an undetermined `warn` finding rather than silence.
- [ ] 5.5 Test that an advisory with no `firstPatchedVersion` still produces a finding.
- [ ] 5.6 Test that `FakeAdvisories::failing()` makes `Audit::run` return `Err`, and that a multi-action lock issues exactly one query.
- [ ] 5.7 Pair every "yields nothing" assertion with a positive assertion over the same fixture, so an inert check cannot satisfy both.

## 6. Verify

- [ ] 6.1 Mutation-test each scenario: break the behavior, confirm the test fails, restore. Record which mutations were run and their results.
- [ ] 6.2 Grep the tree to prove no code path passes a version to an OSV query, and that no OSV endpoint is referenced at all.
- [ ] 6.3 Confirm `src/audit/` is at 5 of 8 files and no `tests/code_health.rs` budget was raised.
- [ ] 6.4 Run `mise run test` and `mise run integ`; both must pass.
