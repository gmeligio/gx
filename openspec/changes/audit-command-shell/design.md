## Context

`gx lint` today owns every "is something wrong here?" verdict, and all 13 of its rules are
offline: they read the working tree and nothing else. The checks this milestone needs —
security advisories, tags that moved under a pin, archived upstream repositories — are not
like that. They query the world, and their verdict changes over time with no edit to the
repository. Folding them into `lint` would break a hermeticity users reasonably assume, and
would make a pre-commit hook depend on the GitHub API.

This change lands the shell: a `gx audit` command, a check-identity type, a report that
renders and serializes, and a trait seam for the GraphQL advisory API. The four checks
(#130–#133) land on top of it as separate changes running in parallel, so the shell's most
important property is that adding a check is a small, conflict-free edit.

Two upstream constraints shape it. `src/diagnostic/` (shipped by the shared-diagnostics
change) already owns `Diagnostic<Id>`, `Report<Id>`, and the `rule_ids!` macro, generic over
rule identity — audit is the second consumer those were generalized for. And the repo
enforces a hard 8-`.rs`-file-per-directory budget, so `src/audit/` must stay small enough
that four follow-on checks fit.

## Goals / Non-Goals

**Goals:**

- `gx audit` runs end to end: reads the lock, runs checks, renders findings, exits 0 or 1.
- `gx audit --json` emits exactly one JSON document, with all progress output suppressed.
- A missing token aborts loudly, before any check runs, naming the variable to set.
- The GraphQL advisory API sits behind a trait with a real adapter and a test fake, so the
  checks that consume it are unit-testable with no network.
- Adding a check is a one-line edit to the check-identity list plus its own new file.
- `gx lint` stays byte-for-byte offline.

**Non-Goals:**

- The four advisory/repository-state checks (#130–#133). This change ships one check,
  `mutable-ref`, computed purely from lock data, so the shell is proven end to end without a
  network dependency in the test suite. It is specified as a requirement in its own right,
  not scaffolding.
- `[audit.rules]` config, per-rule ignores, `--audit-level`. All checks on by default at
  fixed severities.
- Finding provenance ("which file do I edit?"). A lock entry cannot answer that; it depends
  on the `Located`/`Location` rework tracked separately.
- Any new crate dependency. `Cargo.toml`, `Cargo.lock`, and `deny.toml` stay untouched so
  concurrent branches never conflict on them.

## Decisions

### Iterate `gx.lock`, never workflow files

Audit needs `(action_id, version, sha, repository)` per managed action, which is exactly what
the lock records. Re-walking workflows would give audit a second notion of "which actions
exist" that drifts from the scanner's. Reading the lock means composite traversal, job-level
`uses:`, and workflow-root changes all reach audit for free when the scanner learns them.
This also makes audit a pure function from lock → findings, testable with a fixture lock and
no network.

*Alternative considered:* walk workflows for provenance (file:line on each finding). Rejected
— it duplicates discovery, and the provenance it would buy is currently broken for nested
references anyway.

**Enforced, not just intended:** no `src/audit/` file may import the workflow scanner or the
parsed-workflow types. A test in `tests/code_health.rs` asserts this, so a later check cannot
reintroduce traversal by accident.

The same test enforces the mirror-image constraint: no `src/lint/` file may import `reqwest`
or the GitHub API modules. Both directions of the offline/networked split are then a build
failure rather than a convention, which is what the `lint-command` delta requires. One test
covers both because they are one invariant — commands are either offline or networked, and
which one each is may not drift.

### Lock iteration shape

`Lock::entries()` currently yields `(&Spec, &LockEntry)`. A parallel change introduces
`LockedAction<'lock>`, a borrowed per-row view, and changes `entries()` to yield it. To land
cleanly on either, audit defines its own narrow input type and one adapter function that
builds it from whatever `entries()` yields:

```
struct AuditTarget<'lock> {
    id: &'lock ActionId,
    version: &'lock str,      // LockEntry::version_label()
    sha: &'lock CommitSha,
    ref_type: Option<&'lock RefType>,
}
```

Checks take `&AuditTarget`. **This played out as designed:** `LockedAction` landed while
this change was in flight, and adapting to it was a four-line rewrite of `targets()`
(`locked.id()`, `locked.version_label()`, `locked.sha()`, `locked.commit().ref_type`). No
check file changed.

`repository` is deliberately absent: the only shipped check does not read it, and the
project forbids speculative state. The advisory checks will need it as the `package` slug,
so #130 adds one field to this struct and one line to the adapter — the seam is designed
for exactly that.

*Alternative considered:* have checks take `(&Spec, &LockEntry)` directly. Rejected — it
spreads the P4 reconciliation across every check file instead of confining it to one adapter.

### Check identity via `rule_ids!`

`CheckName` is generated by the existing `rule_ids!` macro from one list of
`Variant => "kebab-name"` pairs, which produces the enum, `as_str`, `ALL`, `Display`,
`FromStr`, `Serialize`, and `Deserialize`. Findings are `Diagnostic<CheckName>`; the report
is `Report<CheckName>`, reusing severity counting, `exit_code()`, and summary pluralization.

This is what makes #130–#132 parallel-safe: each adds one line to the list and one file.
Hand-writing a `Display`/`FromStr`/serde triple per check would make every one of those
changes touch the same three impls and conflict.

*Alternative considered:* reuse `lint::RuleName` for audit checks. Rejected — it would let a
user configure an audit check under `[lint.rules]`, and would put networked checks in the
same namespace as offline ones, which is the distinction this whole change exists to draw.

### Token is required, checked before any work

`Settings::github_token` is already `Option<GitHubToken>`, populated from `GITHUB_TOKEN`.
Audit's `run` resolves it first and returns `Error::MissingToken { forge }` when it is
`None`. The error's `Display` names the variable via `Forge::token_env()` — not a literal —
so it reads the same as the sibling resolution errors and cannot go stale when a second
forge lands. The remedies it adds (`gh auth token`, the `env:` snippet, the refusal
sentence) are audit-specific and have no upstream equivalent.

A blank or whitespace-only `GITHUB_TOKEN` counts as absent. CI commonly exports the variable
unconditionally, so `GITHUB_TOKEN=""` is a frequent accident; carrying it as `Some("")` would
satisfy a presence check while authenticating nothing.

This is checked *before* the lock is read and before any check runs, so there is no path on
which audit does partial work and reports a partial-looking clean result. The failure is a
`Result::Err` out of `Command::run`, which `main` propagates as a non-zero process exit —
never a `Report` with zero findings, which is what a JSON consumer would read as "clean".

*Alternative considered:* degrade to unauthenticated like gx's REST paths. Rejected — GitHub's
GraphQL endpoint rejects unauthenticated requests outright, so the only reachable degraded
behavior is "every check silently returns nothing", which is exactly the false-clean this
command exists to prevent.

### GraphQL as a hand-rolled JSON POST behind a trait

`src/infra/github/advisory.rs` (new file; the directory is at 4/8) defines:

- `AdvisoryQuery` — the trait checks depend on.
- `GraphQlAdvisories` — the real adapter. Builds a JSON body `{"query": ..., "variables": ...}`,
  POSTs to `https://api.github.com/graphql` via the existing `reqwest::blocking::Client`
  with the `Authorization: Bearer` header, and deserializes the response. Failures reuse the
  `infra::github::Error` variants (defined in `registry.rs`, re-exported from `mod.rs`).
- `FakeAdvisories` — a bottom `#[cfg(test)] mod fake` in the same file, returning canned
  results. In-file rather than its own file so `src/infra/github/` (6/8 in the integrated
  base, after `resolve.rs` was split into `dates.rs` and `tags.rs`) keeps a free slot for
  the first advisory-consuming check — the thing this seam exists to serve.

Modeled directly on `src/infra/shellcheck/`, which already establishes trait + real adapter +
`#[cfg(test)] fake` in this codebase.

*Alternative considered:* add a GraphQL client crate. Rejected — it would put `Cargo.toml`,
`Cargo.lock`, and `deny.toml` edits into several concurrent branches at once, and a
single-query client is a few dozen lines of `serde` structs.

### Generalize `--json` mode selection in `main.rs`

`main` currently computes `json_mode` with
`matches!(cli.command, Commands::Upgrade { json: true, .. })` and uses it in three places
(log-file suppression, CI notice, the empty-document path). That match arm is replaced by a
`Commands::json_mode(&self) -> bool` method returning the `json` field for each variant that
has one and `false` otherwise. `upgrade`'s behavior is unchanged — the method returns the same
boolean for the same input; only the place the boolean is computed moves.

*Alternative considered:* add a second hardcoded `matches!` arm for audit. Rejected — the
queued `lint --json` change would need a third, and the three call sites would keep drifting.

### One trivial check: `mutable-ref`

To exercise the shell end to end without making the test suite depend on the network, audit
ships one check computed purely from lock data: `mutable-ref` warns when a lock entry
resolved to a **branch** rather than a tag, release, or commit. A branch pin is a real
finding — the SHA recorded today is not what `@main` resolves to tomorrow — and it needs no
API call. It is a genuine check, not a placeholder, so it does not need removing later.

## Automated Test Strategy

**Unit (`src/audit/`, `#[cfg(test)]` at file bottom):**
- `mutable-ref` produces a finding for a `Branch` ref type and none for `Tag`, `Release`,
  or `Commit`.
- Report rendering: clean summary text, per-finding lines, severity counts.
- `to_json()` round-trips: parses back as JSON, carries check name and severity.
- `CheckName` string round-trip through `FromStr`/`Display`.

**Unit (`src/infra/github/advisory.rs`):**
- The GraphQL request body is well-formed JSON with `query` and `variables` keys.
- Response deserialization maps a sample GitHub GraphQL payload to advisory records.
- `FakeAdvisories` satisfies the trait, proving the seam is substitutable — this is the
  infrastructure #130 needs to test its check offline.

**Integration (`tests/integ_audit.rs`, new):**
- Fixture lock with a branch entry → one warning-severity finding, exit 0 (`mutable-ref` is
  fixed at `warn`, and per-rule severity config is a Non-Goal, so there is no branch here
  that yields exit 1).
- Fixture lock with only tag entries → no findings, clean summary.
- Absent lock file, and separately an empty lock file → no findings, exit 0. Both are run
  with a token present, since the token guard precedes the lock read.
- Missing token → `Err(MissingToken)` from `Command::run`, message names `GITHUB_TOKEN`.
- **Lock is the only source:** a fixture whose workflows reference an action absent from the
  lock produces no finding for it. This is the load-bearing invariant, so it gets a test that
  fails if a later change starts reading workflows.

All integration tests use fixture files and the fake adapter — no network, no `#[ignore]`.

**Code health (`tests/code_health.rs`):**
- `audit` added to the two command-module lists (layering + duplicate-fn), registering the
  new layer.
- New bidirectional assertion: no file under `src/audit/` imports the workflow scanner or
  parsed-workflow types, and no file under `src/lint/` imports `reqwest` or the GitHub API
  modules. This is the mechanism the `lint-command` delta requires — the offline/networked
  split becomes a build failure rather than a convention.
- The assertion is itself verified by temporarily introducing a forbidden import and
  confirming the test fails. An enforcement test that cannot fail provides no enforcement,
  and this one is the only thing standing behind a user-facing guarantee.

**Critical path:** lock → targets → checks → report → exit code, and the token guard that
precedes all of it. Both are covered by integration tests against real files.

## Observability

**How failures surface:**

| Failure | Surface | Silent? |
|---|---|---|
| No `GITHUB_TOKEN` | `Err(MissingToken)` from `run`, non-zero exit, message names the variable | No — checked first, before any work |
| GraphQL HTTP/auth/rate-limit error | `Err` out of `run`, non-zero exit, wraps the existing `infra::github::Error` variants (`RateLimited`, `Unauthorized`, `ApiError`) | No |
| Malformed GraphQL response | `Err(ParseResponse)`, non-zero exit | No |
| Check finds a problem | Rendered finding + non-zero exit for error level | No |
| Empty lock | Clean summary, exit 0 | Yes, and correctly so — there is genuinely nothing to audit |

**The silent-failure risk this design targets:** a security command reporting "clean" when it
did not actually check. Every network or configuration failure is an `Err` propagating out of
`Command::run`, never a `Report` with zero findings. The two are structurally different types,
so "failed to check" cannot be rendered as "checked and clean" — including in `--json`, where
the error path prints no JSON document at all rather than an empty one.

**Progress:** audit uses the existing `on_progress` callback, so phase messages reach the
spinner and the local log file exactly as `lint` and `tidy` do — and are suppressed under
`--json` by the same generalized seam.

## Risks / Trade-offs

- **`LockedAction` lands in a parallel change and shifts `Lock::entries()`'s item type** →
  the `AuditTarget` adapter confines the reconciliation to one function; no check file is
  affected. The field list is deliberately a subset of `LockedAction`'s accessors.

- **`src/audit/` could run out of file slots as checks land** → the shell uses 4 of 8 files,
  leaving 4 free for #130–#133 at one file each. Report rendering and JSON live with the
  report type rather than in separate files to protect that headroom.

- **Requiring a token is a harder onboarding step than gx's other commands** → accepted
  deliberately. The alternative is a command that appears to work and certifies nothing. The
  error message carries the fix, and it fires before any work, so the failure is immediate
  rather than partway through a run.

- **In this change specifically, the token guard blocks work that needs no token.** The only
  shipped check, `mutable-ref`, is pure lock data, so until #130–#132 land, `gx audit` demands
  a credential and then runs one offline check. This is a real interim cost, accepted for one
  reason: the guard's value is that it is unconditional. A guard that switches on "do any of
  today's checks need the network?" fails open the moment a networked check is added and the
  condition is not updated — which is exactly the false-clean this command exists to prevent.
  Better a user is asked for a token slightly early than one is silently told "clean" later.

- **`mutable-ref` may fire on repositories that intentionally track a branch** → it is a
  warning, not an error, so it does not fail a build. Per-rule ignores are out of scope here;
  if demand appears, the existing `Level`/`Rule`/`IgnoreTarget` types are already rule-name
  agnostic and can be reused without a new config grammar.

- **Generalizing `json_mode` touches `main.rs`, which several branches edit** → the change is
  a single small method plus replacing one `matches!` expression, chosen to minimize the
  conflict surface, and it is behavior-preserving for `upgrade`.

## Open Questions

None. The JSON field names were the one open item; they are now pinned by the `--json`
requirement in the audit-command spec (per-finding check name, severity, and message, plus
error and warning counts). `gx audit --json` ships first, so it sets the output language and
the queued `lint --json` change conforms to it, rather than audit's contract shifting under
consumers after release.
