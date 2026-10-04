## Context

`audit-command-shell` built `gx audit` and a GitHub GraphQL advisory seam — `AdvisoryQuery` trait, `GraphQlAdvisories` real adapter, `FakeAdvisories` test double — and left it with **no runtime consumer**. `gx audit` today demands a `GITHUB_TOKEN` and spends it on nothing; its one check, `mutable-ref`, is purely offline. This change is the seam's first consumer.

Measured against the live API on 2026-08-06:

```
securityVulnerabilities(ecosystem: ACTIONS, first: 100)
  totalCount 63 · nodes 63 · hasNextPage false · 47 distinct packages
```

The whole ecosystem fits in one page of one query.

### The constraint that shapes everything: OSV fails open here

OSV.dev exposes a `GitHub Actions` ecosystem, but its **version filter silently returns clean for known-vulnerable versions**. Re-verified live against the current advisory data:

| query | result |
|---|---|
| `{"package":{"name":"tj-actions/changed-files","ecosystem":"GitHub Actions"},"version":"45.0.7"}` | `{}` |
| same, versionless | `GHSA-mcph-m25j-8j63`, `GHSA-mrrh-fwg8-r2c3` |
| `{"commit":"0e58ed8671d6b60d0890c21b07f8835ace038e67"}` (the malicious tj-actions commit) | `{}` |

`45.0.7` is squarely inside GHSA-mrrh-fwg8-r2c3's affected range (`<= 45.0.7`) and OSV reports it clean. The cause is structural: Actions advisories carry an `ECOSYSTEM`-type range with no enumerated `versions` array, and OSV has no version-ordering scheme for this ecosystem, so it cannot evaluate the range and returns nothing rather than erroring.

For a security check, a false "clean" is the worst possible outcome — worse than not shipping the check, because the user now believes they were checked. **No code path may pass a version to an OSV query.** Range matching happens locally.

### What the live advisory data actually looks like

Scanning all 63 `vulnerableVersionRange` values informs the parser design:

- **Zero** contain a 40-hex SHA-shaped token. Advisories are keyed by version, never by commit. This is why gx's lock — which records `version` next to `sha` — can do what a workflow-only tool cannot.
- **Partial versions are common and are not valid semver `Version` strings**: `< 1`, `= 1`, `< 2`, `< 3`, `< 4`, `< 6`, `< 17`, `< 41`, `>= 5, < 6.4.0`, `>= 87, < 90`, `>= 2.26.11, < 3.0.0`.
- Comparators are `<`, `<=`, `>=`, `=`, joined by `, ` for two-sided ranges. Package names have **mixed case** (`Azure/setup-kubectl`, `OZI-Project/publish`, `SonarSource/sonarqube-scan-action`).
- One package name is not a slug at all: `https://github.com/pytorch/pytorch/.github/actions/filter-test-configs`.
- No advisory range or patched identifier carries a `v` prefix.

## Goals / Non-Goals

**Goals:**
- An error-level finding for any locked action whose version falls in a published advisory's range, naming GHSA id, severity, range, and first patched version.
- Range matching evaluated locally, unit-tested at real boundaries including the `v` prefix and bare-major forms.
- Exactly one network request per `gx audit` run, independent of lock size.
- No path on which an unparseable version, an uninterpretable range, or a failed lookup renders as "clean".

**Non-Goals:**
- Remediation wording (`gx upgrade <action>` when the fix is in range). A separate change owns it. This change reports `firstPatchedVersion` and stops.
- Advisory caching or offline mode. `gx audit` is explicitly the networked, time-varying command.
- Severity thresholds / per-rule config. `audit-command-shell` ruled this out of scope for the series.
- Pagination. `hasNextPage` is false at 63 of a 100-item page; a `totalCount` overflow is handled as an explicit error, not silently truncated.

## Decisions

### 1. Reshape the seam from per-package to wholesale — do not add a second network path

The existing trait is `fn advisories(&self, package: &str)`, and its query takes `package: String!`. Consuming it as-is means **47 requests to learn 63 facts**, and it scales with lock size.

The change is to the existing seam, not alongside it: `AdvisoryQuery` becomes

```rust
fn all_actions_advisories(&self) -> Result<Vec<Advisory>, Error>;
```

with `Advisory` gaining a `package: String` field (from `package { name }`, which the per-package query had no reason to select). `GraphQlAdvisories` drops the `$package` variable; `FakeAdvisories` keeps its seeded-result and `failing()` shapes.

**Alternatives rejected:** (a) Keep per-package and call it N times — up to 47× the requests to learn the same 63 facts, and a partial failure mid-loop leaves an ambiguous half-audited state where some actions were checked and others were not. (b) Add a second wholesale method beside the existing one — leaves a dead per-package path that no caller uses and that tests must keep alive, which is a second network path to maintain and a second way for a future caller to get the request count wrong. Reshaping the single method keeps exactly one way to reach the advisory API. This is safe to do precisely because the trait has no runtime consumer yet: the signature change is contained to `src/infra/github/` and its own tests.

`totalCount` is selected and compared against the returned node count. If they disagree, the lookup is an **error**, not a truncated success. This is the one place a silent false-clean could re-enter through the back door, so it is closed explicitly rather than left to a future pagination change.

### 2. Match ranges locally with `semver`, on a normalized version — but never let a parse failure mean "clean"

`semver` (already a direct dependency, `semver = "1"`) parses `VersionReq` and `Version`. Two adaptations are needed for the real data:

- **`v` prefix.** gx records `v45.0.7`; advisories write `45.0.7`. Strip a leading `v`/`V` from the locked version before parsing. `Version::precision()` in `src/domain/action/identity.rs` already establishes this convention in gx, so the check follows it rather than inventing a second one.
- **Partial versions.** `semver::Version::parse("40")` fails — but a locked version of `v41` is real, and `< 41` as a `VersionReq` is fine (semver treats a `VersionReq` comparator's missing components as wildcards, which is the correct reading here: `< 41` excludes all of `41.x`). So the *range* needs no padding, but the *locked version* does: `41` → `41.0.0`, `4.2` → `4.2.0`.

**Padding is only sound against an upper bound, and that asymmetry is the last remaining false-clean route.** A `v41` tag points at the 41 line's *head*, not its floor. Against an upper bound this errs toward reporting, which is the safe direction: padded `41.0.0` against `< 41.0.5` matches, and if the real head is `41.9.0` the finding is a false alarm the user can dismiss. Against a **lower** bound it errs toward silence, which is not safe: locked `v2` padded to `2.0.0` against `>= 2.5.0, < 3.0.0` reports clean, even though the 2 line's head may be `2.7.x` and genuinely affected.

Since this change's whole premise is that a false clean is the worst outcome, that path is closed rather than accepted: **a partial locked version whose padded value falls below a lower bound that lies inside its own version line is `undetermined`, not unaffected.** Concretely, when the locked version is partial and the range carries a lower bound whose major (and minor, for a minor-precision pin) equals the locked version's, gx cannot tell which side of the bound the tag actually sits on, so it says so. A fully-specified locked version is unaffected by this rule and matches exactly.

Whether `semver`'s `VersionReq` handles each real comparator form is verified by unit tests over the exact strings harvested from the live API, not assumed. Any form it cannot parse surfaces as an undetermined finding (decision 3), so a parser gap is loud rather than silent.

**Alternative rejected:** hand-rolling a comparator parser. The forms are simple enough to tempt it, but `semver` is already a dependency, already handles pre-release ordering correctly, and a hand-rolled comparator is exactly the sort of code whose bugs are false negatives.

### 3. Undetermined is a third outcome, not a synonym for clean

The binary "affected / not affected" has a hole: a locked version of `main` (branch pin), or a range gx cannot parse, is *neither*. Collapsing it into "not affected" reproduces the OSV failure mode inside gx.

So matching returns three outcomes — affected, unaffected, undetermined — and undetermined produces a finding stating that the action has published advisories whose applicability could not be determined. The user sees it and can judge.

This is deliberately narrow: it only fires for an action that **has** advisories. An unparseable version on an action with no advisories at all is not interesting and stays silent.

Severity of the undetermined finding is `warn`, not `error`: the action may well be fine, and a branch pin already draws an error-adjacent `mutable-ref` warning. Making it `error` would fail builds on an unknown, which is the "false alarm during an incident destroys trust" failure. Making it silent would be the "false clean" failure. `warn` is the honest middle.

### 4. Match packages case-insensitively on the repository, not the action id

Live package names are mixed-case (`Azure/setup-kubectl`), and GitHub slugs are case-insensitive. Comparing case-sensitively would miss a real advisory — a false negative.

The lookup key is the target's **repository**, not its action id: for a nested-path action such as `github/codeql-action/upload-sarif`, the id carries the subpath while the advisory is published against the repository. `AuditTarget` currently has four fields — `id`, `version`, `sha`, `ref_type` — and no `repository`; this change **adds** it, sourced from `entry.commit.repository` by one new line in `targets()`. That is the extension point `audit-command-shell` designed for (`AuditTarget` exists so checks never destructure a lock row themselves), so the check gains a field rather than a competing row type.

Advisories are indexed once into a `HashMap<String, Vec<&Advisory>>` keyed on the lowercased package name, so the check is one hash lookup per locked action rather than a scan of 63 per action.

### 5. One new file, taking one of four remaining slots

`src/audit/` is at 4 of 8 files. This change adds exactly one, `advisory_check.rs`, holding the check and its range matching. The two sibling checks in flight each need one, leaving one spare.

Adding the check name stays a **one-line edit** inside the `rule_ids!` invocation in `check_name.rs` — the macro generates the enum, `as_str`, `ALL`, `Display`, `FromStr`, `Serialize`, and `Deserialize`. No hand-written impls. This is what lets the three checks be developed concurrently without conflicting.

`collect_findings` gains an `&dyn AdvisoryQuery` parameter and one line per check, matching the shape `audit-command-shell` set up.

## Automated Test Strategy

Unit tests, in-file at the bottom under `#[cfg(test)]`, are the critical path. Two layers:

**Range matching** — a table test over the exact boundary strings from the live data, asserting both sides of every boundary:

| range | matches | does not match |
|---|---|---|
| `< 46.0.1` | `46.0.0` | `46.0.1`, `46.0.2` |
| `>= 2.25.0, < 2.37.1` | `2.25.0`, `2.37.0` | `2.24.9`, `2.37.1` |
| `<= 0.24.0` | `0.24.0` | `0.24.1` |
| `<= 45.0.7` | `45.0.7`, `v45.0.7` | `46.0.1` |
| `< 41` | `40.0.0`, `v40` | `41.0.0`, `v41` |
| `>= 87, < 90` | `88.0.0` | `86.0.0`, `90.0.0` |

The `v` prefix is asserted in **both directions** — prefixed locked version against unprefixed range, and the pair `45.0.7` / `v45.0.7` reaching the same verdict — because the bug this guards against is a prefix mismatch reading as "not affected".

**Check behavior**, driven through `FakeAdvisories` (no network):
- A locked action inside a range yields exactly one error finding whose message carries action, version, GHSA id, severity, range, and patched version.
- A locked action outside every range yields nothing.
- An advisory for an unlocked package yields nothing.
- A mixed-case package name still matches.
- A branch-pinned action with advisories yields an undetermined `warn` finding, not silence.
- A branch-pinned action with **no** advisories yields nothing — the narrowing in decision 3.
- Locked `v2` against `>= 2.5.0, < 3.0.0` yields undetermined, **not** unaffected — the lower-bound padding hole from decision 2.
- An advisory with `first_patched: None` still produces a finding.
- `FakeAdvisories::failing()` makes `Audit::run` return `Err`, and the call count proves exactly one query for a multi-action lock and **zero** for an empty lock.

**Deliberately not tested against the live API.** A test that queries GitHub is nondeterministic — it changes verdict when the advisory database changes, which is precisely the property `gx audit` exists to have and a test must not have. The live API was used to *derive* the fixtures; the fixtures are what CI runs. Integration coverage stays at `tests/integ_*` level with a fixture lock, matching how `integ_lint.rs` already works.

**Vacuity discipline.** Every scenario above must fail if the behavior is reverted. Each is mutation-tested during implementation: break the behavior, confirm the test goes red, restore. Specifically at risk of vacuity — and therefore explicitly mutation-checked — are the "yields nothing" cases, which pass trivially if the check never runs. Each is paired with a positive assertion in the same fixture so an inert check cannot satisfy both.

## Observability

**How a vulnerable pin surfaces.** An error-level finding in the standard audit report, rendered through the existing `LintDiag` line and included in `--json` under `findings` with `check: "known-vulnerability"`. Error-level findings drive a nonzero exit, so CI fails.

**Error paths, and whether any can be silent:**

| path | surfaces as | silent? |
|---|---|---|
| No token | `Error::MissingToken`, before any check runs | no — `Err`, no report printed |
| Network failure / rejected credentials | `Error::Advisories`, aborts the run | no — `Err`, no report in either mode |
| GraphQL 200 with `errors` array | same, carrying the server message | no — already handled in the adapter |
| `totalCount` exceeds returned nodes | `Error::Advisories`, aborts | no — explicit check, decision 1 |
| Locked version unparseable | `warn` finding, undetermined | no — decision 3 |
| Advisory range unparseable | `warn` finding, undetermined | no — decision 3 |
| Partial version below a lower bound in its own line | `warn` finding, undetermined | no — decision 2 |
| Action has no advisories | no finding | yes, and correctly so — a successful lookup that found nothing |
| Lock is empty | no finding, no query | yes, and correctly so — nothing to check |

The last two rows are the only silent outcomes, and both are cases where silence is a true statement.

**The distinction the design turns on:** a failed lookup is an `Err`, structurally a different type from a `Report` with zero findings. It cannot be rendered, serialized, or exit-coded as clean, because it never becomes a report at all. Under `--json` a failed run emits no document rather than an empty one — a consumer parsing `findings: []` is therefore always reading a real result.

**Progress.** The existing `on_progress` callback reports the advisory fetch, so a slow network shows as activity rather than a hang.

## Risks / Trade-offs

- **The advisory set outgrows one page (100 items).** At 63 today. → `totalCount` is compared against node count and a mismatch is a hard error, so the failure is a loud abort rather than a silently partial audit. Pagination becomes a small, obvious follow-up when it trips.
- **`semver` cannot parse a comparator form GitHub starts publishing.** → Falls into undetermined (`warn`) rather than "not affected". Unit tests cover every form present in the live data today.
- **Zero-padding a partial locked version (`v41` → `41.0.0`) is an approximation.** A `v41` tag actually points at the 41 line's head, which may be past a `< 41.0.5` boundary. → Against an upper bound, padding to the floor errs toward *reporting*, and a false alarm the user can dismiss beats a false clean. Against a lower bound it would err toward *silence*, so that case is routed to `undetermined` instead (decision 2). Neither direction can produce a silent clean.
- **False alarms during an incident destroy trust.** → Every finding carries the GHSA permalink and the exact affected range, so a user can verify the claim against the advisory in one click rather than taking gx's word for it.
- **A nested-path action's advisory is published against the repository.** → Matching on `repository` rather than the action id handles this; the trade-off is that a vulnerability affecting only one subpath of a repository over-reports to all of them. Advisories are not published at subpath granularity, so this is the only available reading.

## Migration Plan

Additive. `gx audit` already exists, already requires a token, and already emits findings; this adds a check to the existing set. No config, no flags, no output-shape change — `--json` gains rows under the existing `findings` key. Rollback is removing the one line from `rule_ids!` and the one line from `collect_findings`.

The seam reshape is internal: `AdvisoryQuery` had no runtime consumer, so changing its method signature breaks nothing outside `src/infra/github/` and its own tests.

## Open Questions

None blocking. Two settled by the data rather than left open: pagination is not needed at 63 of 100 (and is guarded), and SHA-based lookup is not viable at all (zero SHA-shaped ranges across all 63 advisories, and OSV's commit query returns empty for the known-malicious commit).
