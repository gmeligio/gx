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

The query also selects `advisory.withdrawnAt`. GitHub retains withdrawn entries in this feed; they are excluded before matching, but only after checking the raw node count against `totalCount`. Filtering first would incorrectly reject a complete page containing withdrawn advisories.

### 2. Match ranges locally with `semver`, on a normalized version — but never let a parse failure mean "clean"

`semver` (already a direct dependency, `semver = "1"`) parses `VersionReq` and `Version`. Matching must account for both tag precision and dependency-range semantics:

- **`v` prefix.** gx records `v45.0.7`; advisories write `45.0.7`. Strip a leading `v`/`V` from the locked version before parsing. `Version::precision()` in `src/domain/action/identity.rs` already establishes this convention in gx, so the check follows it rather than inventing a second one.
- **Partial versions.** `semver::Version::parse("40")` fails — but a locked version of `v41` is real, and `< 41` as a `VersionReq` is fine (semver treats a `VersionReq` comparator's missing components as wildcards, which is the correct reading here: `< 41` excludes all of `41.x`). So the *range* needs no padding, but the *locked version* does: `41` → `41.0.0`, `4.2` → `4.2.0`.

**Padding can over-report against an upper bound and under-report against a lower bound.** A `v41` tag points at the 41 line's *head*, not its floor. Padded `41.0.0` matches `< 41.0.5`, even if the actual version is `41.9.0`. Conversely, padded `v2` reads as outside `>= 2.5.0, < 3.0.0`, even though the actual version may be `2.7.x`.

A partial pin that could overlap an affected range must not become unaffected merely because its zero-padded value does not match. This includes equality ranges: `v2` against `= 2.5.0` is undetermined, while `v3` and `v2.4` are disjoint. Existing upper-bound conservatism is retained: if the padded version matches, the advisory is reported.

Whether `semver`'s `VersionReq` handles each real comparator form is verified by unit tests over the exact strings harvested from the live API, not assumed. Any form it cannot parse surfaces as an undetermined finding (decision 3), so a parser gap is loud rather than silent.

`VersionReq::matches` excludes prereleases unless a comparator explicitly opts into the corresponding prerelease version. A failed match due to that dependency-selection rule is not evidence of safety. When prerelease applicability cannot be established, report undetermined rather than unaffected.

**Alternative rejected:** a new comparator parser. Reuse `semver` for syntax and supported matches, retaining an explicit uncertainty outcome instead of reproducing a package manager's entire range language.

### 3. Undetermined is a third outcome, not a synonym for clean

The binary "affected / not affected" has a hole: a locked version of `main` (branch pin), or a range gx cannot parse, is *neither*. Collapsing it into "not affected" reproduces the OSV failure mode inside gx.

So matching returns three outcomes — affected, unaffected, undetermined — and undetermined produces a finding stating that the action has published advisories whose applicability could not be determined. The user sees it and can judge.

This is deliberately narrow: it only fires for an action that **has** advisories. An unparseable version on an action with no advisories at all is not interesting and stays silent.

Severity of the undetermined finding is `warn`, not `error`: the action may well be fine, and a branch pin already draws an error-adjacent `mutable-ref` warning. Making it `error` would fail builds on an unknown, which is the "false alarm during an incident destroys trust" failure. Making it silent would be the "false clean" failure. `warn` is the honest middle.

### 4. Match repository advisories and action-scoped packages

Live package names are mixed-case (`Azure/setup-kubectl`), and GitHub slugs are case-insensitive. Comparing case-sensitively would miss a real advisory — a false negative.

Repository-wide advisories look up the target's **repository**, not its full action id. For example, an advisory for `github/codeql-action` also covers `github/codeql-action/upload-sarif`. `AuditTarget.repository` is sourced from the locked commit.

Some packages explicitly name an action URL, including `https://github.com/pytorch/pytorch/.github/actions/filter-test-configs`. Normalize recognized HTTPS GitHub URLs to repository and optional subpath, preserving case-sensitive subpaths. An action-scoped advisory matches only that action; it must neither disappear nor spread to every action in the repository. Foreign hosts are not GitHub package identities.

Advisories are indexed once, with repository and action scopes kept distinct. The same advisory must not be emitted twice when the target is a repository-root action.

### 5. Separate matching from finding construction

`advisory_check.rs` owns package lookup and findings; `advisory_range.rs` owns version-range matching. The split keeps each file within existing budgets as regression coverage grows. These additions take `src/audit/` from 4 files to 6, against a budget of 8; no budget is raised.

Adding the check name stays a **one-line edit** inside the `rule_ids!` invocation in `check_name.rs` — the macro generates the enum, `as_str`, `ALL`, `Display`, `FromStr`, `Serialize`, and `Deserialize`. No hand-written impls. This is what lets the three checks be developed concurrently without conflicting.

`collect_findings` gains an `&dyn AdvisoryQuery` parameter and one line per check, matching the shape `audit-command-shell` set up.

### 6. Test production command composition

The audit command accepts its advisory source rather than constructing it inside the only executable pipeline. Production constructs the real adapter; tests supply canned data and call the same `Command::run`. Do not hand-assemble the token/check/report sequence in test helpers.

The CLI checks audit credentials immediately after argument parsing, before repository discovery, configuration loading, or log setup. The command also checks credentials when invoked directly. This keeps malformed files from hiding the missing-token error while preserving `--help` and non-audit behavior.

Subprocess tests use a fixture-owned temporary directory to check log-file side effects. A JSON run must create no audit log; a human-mode run must create one, proving that the assertion examines the actual log location.

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
- Prerelease exclusions and partial pins overlapping equality ranges never silently report unaffected; disjoint partial pins remain unaffected.
- GitHub URL packages match the named subpath, not unrelated subpaths or foreign-host URLs.
- Withdrawn advisories produce no findings while active advisories still do; raw response completeness is checked before filtering.
- An advisory with `first_patched: None` still produces a finding.
- `FakeAdvisories::failing()` makes `Audit::run` return `Err`, and the call count proves exactly one query for a multi-action lock and **zero** for an empty lock.

**Deliberately not tested against the live API.** A test that queries GitHub is nondeterministic — it changes verdict when the advisory database changes, which is precisely the property `gx audit` exists to have and a test must not have. The live API was used to *derive* the fixtures; the fixtures are what CI runs. Integration coverage stays at `tests/integ_*` level with a fixture lock, matching how `integ_lint.rs` already works.

**Vacuity discipline.** Every scenario above must fail if the behavior is reverted. Each is mutation-tested during implementation: break the behavior, confirm the test goes red, restore. Specifically at risk of vacuity — and therefore explicitly mutation-checked — are the "yields nothing" cases, which pass trivially if the check never runs. Each is paired with a positive assertion in the same fixture so an inert check cannot satisfy both.

## Observability

**How a vulnerable pin surfaces.** An error-level finding in the standard audit report, rendered through the existing `LintDiag` line and included in `--json` under `findings` with `check: "known-vulnerability"`. Error-level findings drive a nonzero exit, so CI fails.

**Error paths, and whether any can be silent:**

| path | surfaces as | silent? |
|---|---|---|
| No token | `Error::MissingToken`, before repository configuration is loaded | no — `Err`, no report printed |
| Network failure / rejected credentials | `Error::Advisories`, aborts the run | no — `Err`, no report in either mode |
| GraphQL 200 with `errors` array | same, carrying the server message | no — already handled in the adapter |
| `totalCount` exceeds returned nodes | `Error::Advisories`, aborts | no — explicit check, decision 1 |
| Locked version unparseable | `warn` finding, undetermined | no — decision 3 |
| Advisory range unparseable | `warn` finding, undetermined | no — decision 3 |
| Partial version ambiguously overlapping a range | `warn` finding, undetermined | no — decision 2 |
| Prerelease applicability not established | `warn` finding, undetermined | no — decision 2 |
| Advisory withdrawn | no finding | yes — retracted claims are excluded |
| Action has no advisories | no finding | yes, and correctly so — a successful lookup that found nothing |
| Lock is empty | no finding, no query | yes, and correctly so — nothing to check |

An advisory proven not to cover the locked version also produces no finding.

**The distinction the design turns on:** a failed lookup is an `Err`, structurally a different type from a `Report` with zero findings. It cannot be rendered, serialized, or exit-coded as clean, because it never becomes a report at all. Under `--json` a failed run emits no document rather than an empty one — a consumer parsing `findings: []` is therefore always reading a real result.

**Progress.** The existing `on_progress` callback reports the advisory fetch, so a slow network shows as activity rather than a hang.

## Risks / Trade-offs

- **The advisory set outgrows one page (100 items).** At 63 today. → `totalCount` is compared against node count and a mismatch is a hard error, so the failure is a loud abort rather than a silently partial audit. Pagination becomes a small, obvious follow-up when it trips.
- **`semver` cannot parse a comparator form GitHub starts publishing.** → Falls into undetermined (`warn`) rather than "not affected". Unit tests cover every form present in the live data today.
- **Zero-padding a partial locked version (`v41` → `41.0.0`) is an approximation.** Against an upper bound, padding can over-report. Against a lower bound or equality range, ambiguous overlap is undetermined instead of unaffected (decision 2).
- **False alarms during an incident destroy trust.** → Every finding carries the GHSA permalink and the exact affected range, so a user can verify the claim against the advisory in one click rather than taking gx's word for it.
- **A repository-wide advisory may concern only part of that repository.** Match its published scope; when the package explicitly names a subpath, preserve that narrower scope.

## Migration Plan

Additive. `gx audit` already exists, already requires a token, and already emits findings; this adds a check to the existing set. No config, no flags, no output-shape change — `--json` gains rows under the existing `findings` key. Rollback is removing the one line from `rule_ids!` and the one line from `collect_findings`.

The seam reshape is internal: `AdvisoryQuery` had no runtime consumer, so changing its method signature breaks nothing outside `src/infra/github/` and its own tests.

## Open Questions

None blocking. Two settled by the data rather than left open: pagination is not needed at 63 of 100 (and is guarded), and SHA-based lookup is not viable at all (zero SHA-shaped ranges across all 63 advisories, and OSV's commit query returns empty for the known-malicious commit).
