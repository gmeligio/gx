## 1. Reshape the advisory seam to wholesale

- [x] 1.1 Add `package: String` to `Advisory` in `src/infra/github/advisory.rs`, populated from `package { name }`.
- [x] 1.2 Replace `ADVISORY_QUERY` with the unparameterized ecosystem query selecting `totalCount` and `package { name }`; drop the `$package` variable and the `Variables` struct.
- [x] 1.3 Change the `AdvisoryQuery` trait method to `all_actions_advisories(&self) -> Result<Vec<Advisory>, Error>`; update `GraphQlAdvisories`.
- [x] 1.4 In `interpret`, error when `totalCount` is greater than the number of returned nodes, so a truncated page can never read as a complete clean result. (Only this direction: fewer advisories than `totalCount` means data was withheld; the reverse cannot occur and is not worth an error path.)
- [x] 1.5 Update `FakeAdvisories` to the new method, keeping `new(...)` and `failing()`, and add a call counter so tests can assert how many queries were issued.
- [x] 1.6 Update the existing advisory unit tests to the new shape; add one asserting the `totalCount` guard errors rather than returning a short list.

## 2. Add `repository` to the audit target

- [x] 2.1 Add a `repository: &'lock Repository` field to `AuditTarget` in `src/audit/target.rs`. `AuditTarget` today has four fields (`id`, `version`, `sha`, `ref_type`) and no `repository` — this is a new field, not a restoration.
- [x] 2.2 Populate it in `targets()` from `entry.commit.repository`, one added line in the existing map closure.
- [x] 2.3 Confirm no other check's file is touched and `mutable_ref` is unchanged.

## 3. Register the check name

- [x] 3.1 Add `KnownVulnerability => "known-vulnerability",` as a single line inside the `rule_ids!` invocation in `src/audit/check_name.rs`. No hand-written `Display`/`FromStr`/serde.

## 4. Implement range matching

- [x] 4.1 Create `src/audit/advisory_check.rs` (taking `src/audit/` from 4 files to 5, against a budget of 8).
- [x] 4.2 Implement local range matching returning a three-way outcome — affected, unaffected, undetermined — using `semver::VersionReq` for the range and `semver::Version` for the locked version.
- [x] 4.3 Normalize the locked version before parsing: strip a leading `v`/`V`, zero-pad partial versions (`41` → `41.0.0`, `4.2` → `4.2.0`), following the convention `Version::precision()` already sets.
- [x] 4.4 Return undetermined — never unaffected — when the locked version is partial and the range carries a lower bound inside that version's own line (locked `v2` vs `>= 2.5.0, < 3.0.0`), since the tag may resolve either side of the bound.
- [x] 4.5 Return undetermined when the locked version or the advisory range fails to parse.

## 5. Implement the check

- [x] 5.1 Index advisories once into a map keyed on the lowercased package name; look up by the target's `repository`, case-insensitively.
- [x] 5.2 Emit an error-level finding naming action, locked version, GHSA id, severity, affected range, advisory permalink, and first patched version when present.
- [x] 5.3 Emit a `warn`-level undetermined finding only when the action has advisories whose applicability could not be decided; stay silent for actions with no advisories.

## 6. Wire the check into the command

- [x] 6.1 Thread `&dyn AdvisoryQuery` through `collect_findings` in `src/audit/mod.rs` and register the check with one line.
- [x] 6.2 Skip the advisory query entirely when the lock has no entries, so an empty lock cannot fail for a network reason.
- [x] 6.3 Add the `Error` variant for a failed lookup; construct `GraphQlAdvisories` in `Audit::run` after the existing token guard.
- [x] 6.4 Report the advisory fetch through the existing `on_progress` callback.

## 7. Test

- [x] 7.1 Table-test range matching over every boundary in the design's table, asserting both sides of each: `< 46.0.1`, `>= 2.25.0, < 2.37.1`, `<= 0.24.0`, `<= 45.0.7`, `< 41`, `>= 87, < 90`.
- [x] 7.2 Assert the `v` prefix in both directions, including that `45.0.7` and `v45.0.7` reach the same verdict.
- [x] 7.3 Test via `FakeAdvisories`: affected yields one error finding carrying every required element including the permalink; unaffected yields nothing; an advisory for an unlocked package yields nothing; a mixed-case package name still matches.
- [x] 7.4 Test that a branch-pinned action with advisories yields an undetermined `warn` finding, and that one with no advisories yields nothing.
- [x] 7.5 Test that locked `v2` against `>= 2.5.0, < 3.0.0` is undetermined rather than unaffected.
- [x] 7.6 Test that an advisory with no `firstPatchedVersion` still produces a finding.
- [x] 7.7 Test that `FakeAdvisories::failing()` makes `Audit::run` return `Err`, that a multi-action lock issues exactly one query, and that an empty lock issues zero.
- [x] 7.8 Pair every "yields nothing" assertion with a positive assertion over the same fixture, so an inert check cannot satisfy both.

## 8. Verify

- [x] 8.1 Mutation-test each scenario: break the behavior, confirm the test fails, restore. Record which mutations were run and their results.
- [x] 8.2 Grep `src/` to prove no code path passes a version to an OSV query and that no OSV endpoint is referenced in code. (Prose references to OSV in the planning artifacts are expected and do not count.)
- [x] 8.3 Confirm `src/audit/` holds 5 `.rs` files and that no numeric budget in `tests/code_health.rs` was raised.
- [x] 8.4 Run `mise run test` and `mise run integ`; both must pass.
