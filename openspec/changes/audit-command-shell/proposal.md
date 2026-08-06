## Why

The GitHub Actions ecosystem tells you to pin SHAs, then goes blind on SHA pins. Dependabot
"will not create alerts for actions pinned to SHA values"; Renovate skips a bare SHA with no
version comment. In the tj-actions compromise (GHSA-mrrh-fwg8-r2c3), *tag* users were
auto-remediated when the tags were repointed — the SHA-pinned users were the ones left
running malicious code. Pinning converts a transient compromise into a permanent one unless
something re-checks the pin.

gx is unusually well placed to close this: `gx.lock` already records `sha` + `version` +
`repository` per action, and advisories are keyed by **version** — exactly the mapping a
workflow-only tool does not have. This change lands the command shell and the network seam
so the individual checks can follow independently.

## What Changes

- New `gx audit` subcommand: a networked, time-varying security check over the actions
  recorded in `gx.lock`. Reports findings, exits 1 when any error-level finding survives.
- New `gx audit --json` mode emitting a single JSON document on stdout, for CI consumption.
- A GitHub GraphQL advisory client behind a trait seam (`src/infra/github/advisory.rs`),
  hand-rolled as a JSON POST over the existing `reqwest` blocking client — no new dependency.
- **`gx audit` requires a GitHub token and fails loudly without one.** GraphQL rejects
  unauthenticated requests; a security command that silently reports "clean" is worse than
  no command at all.
- Audit iterates `gx.lock` entries. It does **not** walk workflow files — discovery stays
  the scanner's job, so future discovery work (composite traversal, job-level `uses:`)
  reaches audit for free.
- `gx lint` is unchanged and remains 100% offline.
- `--json` mode selection in `src/main.rs` is generalized from a hardcoded `Commands::Upgrade`
  match to a per-command property, so `upgrade` behavior is preserved and `audit` (and later
  `lint --json`) reuse one seam.

Not in this change: the individual checks (advisory lookup, tag-moved, archived-repo,
remediation), `[audit.rules]` config, per-rule ignores, and `--audit-level`. Audit ships with
one trivially verifiable check so the shell is exercised end to end.

## Capabilities

### New Capabilities
- `audit-command`: `gx audit` — a networked security audit over the locked action set,
  its token requirement, its exit-code contract, and its `--json` output.

### Modified Capabilities
- `lint-command`: adds an explicit offline guarantee. `gx lint` performs no network I/O;
  this is now a stated requirement rather than an accident of implementation, because a
  sibling command that *does* use the network makes the distinction user-visible.

## Impact

- New `src/audit/` module (command, check identity, report rendering).
- New `src/infra/github/advisory.rs` — GraphQL client trait, real adapter, test fake.
- `src/main.rs`: new subcommand registration, `GxError` variant, generalized JSON-mode seam.
- `tests/code_health.rs`: `audit` added to the two command-module lists (registering a new
  layer, not raising a budget).
- `README.md`: `gx audit` added to the command list with its token requirement.
- No changes to `Cargo.toml`, `Cargo.lock`, or `deny.toml` — no new dependency.
