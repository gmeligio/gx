## Why

Pinning an action to a SHA converts a transient upstream compromise into a permanent one. GitHub's own tooling goes blind here: Dependabot "will not create alerts for actions pinned to SHA values", and Renovate skips a bare SHA with no version comment. The tj-actions compromise (GHSA-mrrh-fwg8-r2c3) is the worked example — tag users were auto-remediated when tags were repointed; the SHA-pinned users were the ones left running malicious code.

`gx.lock` records `version` alongside `sha`, so gx holds the SHA to version mapping that advisories are keyed by and a workflow-only tool does not have. `gx audit` already demands a GitHub token and spends it on nothing. This change makes it earn that token: every locked action is checked against the GitHub Advisory Database and a vulnerable pin becomes an error-level finding.

This meets the relevance gate on both counts — it adds user-facing behavior (a new class of finding, a new nonzero exit condition) and it introduces a domain concept, "this pin is known-vulnerable", that changes what users can do.

## What Changes

- New audit check `known-vulnerability`, error level, on by default. For each locked action whose version falls inside a published advisory's affected range, emit a finding naming the GHSA id, the severity, the affected range, and the first patched version.
- `gx audit` performs one network call: the full `ACTIONS` ecosystem advisory set, fetched wholesale rather than per action. Measured live: 63 advisories over 47 packages on a single page, so per-action querying would be up to 47 requests to learn the same 63 facts.
- Version-range matching is done **locally** with the `semver` crate. No code path passes a version to OSV — see the design for the verified fail-open behavior that forces this.
- A failed advisory lookup is an error that aborts the run. It is never rendered as a clean report.
- `gx audit --json` gains `known-vulnerability` findings under its existing `findings` key. The JSON shape is unchanged.

## Capabilities

### New Capabilities
- `audit-advisories`: checking locked actions against the GitHub Advisory Database — which actions are checked, what a finding says, how version ranges are matched, and how a failed or unavailable lookup is reported.

### Modified Capabilities

None. The `audit` capability's spec is introduced by the in-flight `audit-command-shell` change and is not yet in `openspec/specs/`; this change adds a sibling capability rather than editing an unarchived one.

## Impact

- **New file** `src/audit/advisory_check.rs` — the check plus local range matching. Takes one of the four remaining `src/audit/` file slots (4 of 8 used).
- **Modified** `src/audit/check_name.rs` — one line inside `rule_ids!`.
- **Modified** `src/audit/mod.rs` — `collect_findings` gains an `&dyn AdvisoryQuery` parameter; `Audit::run` constructs the real adapter; a new `Error` variant for a failed lookup.
- **Modified** `src/audit/target.rs` — re-adds the `repository` field that `audit-command-shell` dropped as unused, plus its one adapter line.
- **Modified** `src/infra/github/advisory.rs` and `advisory_fake.rs` — the seam becomes a wholesale `all_actions_advisories()` returning `Advisory` records that carry their package name.
- **Dependencies**: none added. `semver = "1"` is already a direct dependency.
- **Not in scope**: remediation wording (`gx upgrade <action>`), which is a separate change. This change reports `firstPatchedVersion` and stops there.
