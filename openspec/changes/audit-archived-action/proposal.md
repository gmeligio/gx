## Why

An archived GitHub repository accepts no commits and no pull requests while archived.
If a vulnerability is found in an action published from one, maintainers cannot publish a
patch unless they resume maintenance. Migrating to a maintained alternative is the
recommended remediation rather than waiting for an upgrade.

This is not hypothetical. `actions/setup-ruby`, `actions/create-release`, and
`actions/upload-release-asset` are archived today and still appear in workflows across the
ecosystem. A user pinned to one of them has a dependency that is, from a security
standpoint, frozen while archived.

Planned advisory-based checks answer "is this action vulnerable *right now*".
This check gives users lead time to migrate before a vulnerability is disclosed.

## What Changes

- New `archived-action` audit check. For each locked action it issues one REST call,
  `GET /repos/{owner}/{repo}`, and reports at **warn** severity when the response says the
  repository is archived.
- The finding includes the date of the repository's last push, so the user can see how
  stale the dependency is, and it names migration — not upgrade — as the remedy, because
  new fixes cannot be published while the repository remains archived.
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
  lookup behaves. Also gains an `ADDED` requirement placing archived-repository metadata
  in audit rather than offline lint, without changing the existing mutable-ref audit check.

## Impact

- `src/audit/check_name.rs`: one line added to the `rule_ids!` list.
- New `src/audit/archived.rs`: the check itself (one new file; `src/audit/` moves 4 → 5 of
  its 8-file budget).
- New `src/infra/github/repo_meta.rs`: the metadata seam, its REST adapter, and its test
  double. **One file, not two** — `src/infra/github/` holds 7 files against an 8-file
  budget, so one new file lands exactly at 8 and a separate `repo_meta_fake.rs` would break
  the budget. `advisory.rs` already keeps its own fake inline for the same reason. See
  design Decision 2.
- `src/audit/mod.rs`: the check is wired into the run, and the command constructs the real
  adapter.
- `src/audit/target.rs`: **no change.** The repository is derived from the action id via
  `ActionId::base_repo()` rather than carried as a new field — see design Decision 1.
- No changes to `Cargo.toml`, `Cargo.lock`, or `deny.toml` — no new dependency. The check
  reuses the existing `reqwest` blocking client through `Registry`.
- No numeric budget in `tests/code_health.rs` is raised.
