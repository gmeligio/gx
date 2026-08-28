## Why

An archived GitHub repository accepts no commits and no pull requests. If a vulnerability
is found in an action published from one, there will never be a patched version — the
`firstPatchedVersion` an advisory would normally point at does not exist and cannot come to
exist. Upgrading is not a remediation; migrating off the action is the only one.

This is not hypothetical. `actions/setup-ruby`, `actions/create-release`, and
`actions/upload-release-asset` are archived today and still appear in workflows across the
ecosystem. A user pinned to one of them has a dependency that is, from a security
standpoint, permanently frozen — and nothing in their toolchain tells them so.

The advisory-based checks in this milestone answer "is this action vulnerable *right now*".
This check answers a different and prior question: "if it becomes vulnerable, can it be
fixed at all?" A user who learns the answer is no wants to know before the advisory lands,
not after.

## What Changes

- New `archived-action` audit check. For each locked action it issues one REST call,
  `GET /repos/{owner}/{repo}`, and reports at **warn** severity when the response says the
  repository is archived.
- The finding includes the date of the repository's last push, so the user can see how
  stale the dependency is, and it names migration — not upgrade — as the remedy, because
  upgrading is not available.
- Subpath actions resolve to their base repository before the call
  (`github/codeql-action/upload-sarif` → `github/codeql-action`), since the API endpoint is
  repo-level and the subpath would 404.
- A new repository-metadata seam (trait + real REST adapter + test fake) so the check is
  unit-testable with no network, matching the shape `audit-command-shell` established for
  advisories.
- **A failed metadata lookup is an error, never a silent "not archived".** Consistent with
  the token guard: audit must not convert "could not check" into an affirmative clean.

Not in this change: `[audit.rules]` config, per-check ignores, and `--audit-level` — all
deferred by `audit-command-shell` and unchanged here. No caching layer: one call per locked
action is proportionate, and a cache is speculative until a real lock proves it slow.

## Capabilities

### New Capabilities

None. This change adds requirements to `audit-command`, the capability introduced by the
`audit-command-shell` change.

### Modified Capabilities

- `audit-command`: gains the `archived-action` check as an `ADDED` requirement — its
  severity, the content of its finding, how subpath actions resolve, and how a failed
  lookup behaves. Also gains an `ADDED` requirement stating explicitly that audit's scope
  is dependency *health*, not vulnerabilities alone. No existing requirement changes
  meaning.

## Impact

- `src/audit/check_name.rs`: one line added to the `rule_ids!` list.
- New `src/audit/archived.rs`: the check itself (one new file; `src/audit/` moves 4 → 5 of
  its 8-file budget).
- New `src/infra/github/repo_meta.rs`: the metadata seam, its REST adapter, and its test
  double. **One file, not two** — `src/infra/github/` holds 6 files against an 8-file
  budget, and the advisory-consuming check developed in parallel is likely to want a slot
  there. Adding one file leaves 7/8; adding a separate `repo_meta_fake.rs` would leave 8/8
  with no headroom. See design Decision 2.
- `src/audit/mod.rs`: the check is wired into the run, and the command constructs the real
  adapter.
- `src/audit/target.rs`: **no change.** The repository is derived from the action id via
  `ActionId::base_repo()` rather than carried as a new field — see design Decision 1.
- No changes to `Cargo.toml`, `Cargo.lock`, or `deny.toml` — no new dependency. The check
  reuses the existing `reqwest` blocking client through `Registry`.
- No numeric budget in `tests/code_health.rs` is raised.
