## 1. JSON-mode generalization (behavior-preserving, lands first)

Isolated from the rest because it is the only work that can regress a shipped command.

- [x] 1.1 Add `Commands::json_mode(&self) -> bool` in `src/main.rs`, returning the `json`
      field for each variant that has one and `false` otherwise
- [x] 1.2 Replace the `matches!(cli.command, Commands::Upgrade { json: true, .. })`
      expression with a call to it
- [x] 1.3 Verify `upgrade` is unchanged by running `tests/integ_upgrade.rs` before and after
      the refactor and confirming identical results; `--json` must still suppress the CI
      notice, the log file, and the spinner, and still emit the empty document when
      `.github` is missing

## 2. Advisory network seam

Satisfies: "Advisory lookups go through a substitutable seam".

- [x] 2.1 Create `src/infra/github/advisory.rs` with the `AdvisoryQuery` trait, the advisory
      record type, and the GraphQL request/response `serde` structs
- [x] 2.2 Implement `GraphQlAdvisories` — JSON POST to `https://api.github.com/graphql` over
      the existing `reqwest::blocking::Client`, `Authorization: Bearer` header, reusing the
      `infra::github::Error` variants (defined in `registry.rs`, re-exported from `mod.rs`)
      so auth, rate-limit, and malformed responses stay `Err`
- [x] 2.3 Add `FakeAdvisories` under `#[cfg(test)]` returning canned results
- [x] 2.4 Export the seam from `src/infra/github/mod.rs`
- [x] 2.5 Unit tests: request body shape, response deserialization, and a check-shaped
      consumer running against `FakeAdvisories` with no network

## 3. Check identity and findings

Satisfies: "Checks are identified by a stable kebab-case name",
"gx audit --json emits one machine-readable document".

- [x] 3.1 Create `src/audit/check_name.rs` defining `CheckName` via `crate::rule_ids!` with
      the single `MutableRef => "mutable-ref"` entry
- [x] 3.2 Create `src/audit/report.rs`: `Finding = Diagnostic<CheckName>`,
      `Report = crate::diagnostic::Report<CheckName>`, `impl CommandReport for Report`
      (render via `OutputLine::LintDiag`, `exit_code`, clean summary text)
- [x] 3.3 Add `Report::to_json()` emitting one document with per-finding check name,
      severity, and message, plus error and warning counts
- [x] 3.4 Unit tests: rendering clean and non-clean, JSON field names and shape,
      `CheckName` round-trips to and from the literal string `mutable-ref`
- [x] 3.5 Confirm `mutable-ref` is rejected as a `[lint.rules]` key (lint's `RuleName`
      parse already errors on unknown names — assert it, do not add code)

## 4. Lock iteration and the mutable-ref check

Satisfies: "Audit the locked action set with gx audit",
"mutable-ref check reports lock entries pinned to a branch".

- [x] 4.1 Create `src/audit/target.rs` with `AuditTarget<'lock>` and the single adapter
      function building targets from `Lock::entries()`
- [x] 4.2 Implement the `mutable-ref` check: warn when a target's `ref_type` is `Branch`
- [x] 4.3 Unit tests: `Branch` yields a warning-severity finding naming the action;
      `Tag`, `Release`, and `Commit` each yield none

## 5. Command shell

Satisfies: "Audit the locked action set with gx audit",
"A missing GitHub token is a loud, actionable failure".

- [x] 5.1 Create `src/audit/mod.rs`: `Audit` struct, `Error` enum with `MissingToken` whose
      message names `GITHUB_TOKEN`, and `impl Command for Audit`
- [x] 5.2 Resolve the token first in `run`, before reading the lock or running any check;
      return `Err(MissingToken)` when absent
- [x] 5.3 Run checks over the targets and return `Report::from_diagnostics(...)`
- [x] 5.4 Register the module in `src/lib.rs`

## 6. CLI registration

- [x] 6.1 Add `Commands::Audit { json: bool }`, the `cmd_name` arm, and the `use`/`GxError`
      entries in `src/main.rs`
- [x] 6.2 Add the dispatch arm running audit, printing JSON under `--json` and rendered
      lines otherwise, suppressing spinner and log file in JSON mode
- [x] 6.3 Add a named test in `tests/integ_audit.rs` asserting that with `--json` and no
      token, stdout is empty — no JSON document at all, not an empty one and not one with
      an error field beside zero findings. This is the assertion behind the spec's
      strongest claim, so it gets a test, not a manual check

## 7. Tests and guardrails

- [x] 7.1 Create `tests/integ_audit.rs`: branch-entry lock (one warning, exit 0), tag-only
      lock (clean, exit 0), absent lock file, empty lock file, missing token
- [x] 7.2 Add the lock-is-the-only-source test. The unlocked action in the fixture MUST be a
      branch reference — one that *would* yield a `mutable-ref` finding if audit walked
      workflows — so the test actually fails if a later change reintroduces traversal
- [x] 7.3 Add `audit` to the two command-module lists in `tests/code_health.rs`
- [x] 7.4 Add the bidirectional code-health assertion: no `src/audit/` file imports the
      workflow scanner or parsed-workflow types, and no `src/lint/` file imports `reqwest`
      or the GitHub API modules
- [x] 7.5 Confirm the new assertion actually fires — temporarily add a forbidden import,
      see the test fail, then revert. A guardrail that cannot fail is not a guardrail

## 8. Documentation and verification

- [ ] 8.1 Add `gx audit` to the README command list, noting the `GITHUB_TOKEN` requirement
- [ ] 8.2 Leave `docs/demo.tape` unchanged. Decision recorded here rather than deferred: the
      tape is a single scripted `gx tidy` walkthrough, not a command inventory, and `gx audit`
      cannot be recorded there without a live token in the recording environment. Done when
      the tape is confirmed untouched in the final diff
- [ ] 8.3 Run `mise run test` and `mise run integ`; both must pass
- [ ] 8.4 Verify directory budgets: `src/audit/` contains exactly 4 files (leaving 4 free,
      one for each of #130–#133), and `src/infra/github/` stays within 8
