## 1. Network seam

- [ ] 1.1 Create `src/infra/github/advisory.rs` with the `AdvisoryQuery` trait, the advisory
      record type, and the GraphQL request/response `serde` structs
- [ ] 1.2 Implement `GraphQlAdvisories` — JSON POST to `https://api.github.com/graphql` over
      the existing `reqwest::blocking::Client`, `Authorization: Bearer` header, reusing
      `registry::Error` variants for status classification
- [ ] 1.3 Add `FakeAdvisories` under `#[cfg(test)]` returning canned results
- [ ] 1.4 Export the seam from `src/infra/github/mod.rs`
- [ ] 1.5 Unit tests: request body shape, response deserialization, fake satisfies the trait

## 2. Audit check identity and findings

- [ ] 2.1 Create `src/audit/check_name.rs` defining `CheckName` via `crate::rule_ids!` with
      the single `MutableRef => "mutable-ref"` entry
- [ ] 2.2 Create `src/audit/report.rs`: `Finding = Diagnostic<CheckName>`,
      `Report = crate::diagnostic::Report<CheckName>`, `impl CommandReport for Report`
      (render via `OutputLine::LintDiag`, `exit_code`, clean summary text)
- [ ] 2.3 Add `Report::to_json()` emitting one document with findings and counts
- [ ] 2.4 Unit tests: rendering clean and non-clean, JSON round-trip, `CheckName` string
      round-trip

## 3. Lock iteration and the mutable-ref check

- [ ] 3.1 Create `src/audit/target.rs` with `AuditTarget<'lock>` and the single adapter
      function building targets from `Lock::entries()`
- [ ] 3.2 Implement the `mutable-ref` check: warn when a target's `ref_type` is `Branch`
- [ ] 3.3 Unit tests: `Branch` yields a finding; `Tag`, `Release`, `Commit` yield none

## 4. Command shell

- [ ] 4.1 Create `src/audit/mod.rs`: `Audit` struct, `Error` enum with `MissingToken` whose
      message names `GITHUB_TOKEN`, and `impl Command for Audit`
- [ ] 4.2 Resolve the token first in `run`, before reading the lock or running any check;
      return `Err(MissingToken)` when absent
- [ ] 4.3 Run checks over the targets, collect findings, return
      `Report::from_diagnostics(...)`
- [ ] 4.4 Register the module in `src/lib.rs`

## 5. CLI registration and JSON generalization

- [ ] 5.1 Add `Commands::Audit { json: bool }`, the `cmd_name` arm, and the `use`/`GxError`
      entries in `src/main.rs`
- [ ] 5.2 Replace the `matches!(cli.command, Commands::Upgrade { json: true, .. })`
      expression with a `Commands::json_mode(&self) -> bool` method covering every variant
- [ ] 5.3 Add the dispatch arm running audit, printing JSON under `--json` and rendered lines
      otherwise, suppressing spinner and log file in JSON mode
- [ ] 5.4 Verify `upgrade` behavior is unchanged: `--json` still suppresses the CI notice,
      log file, and spinner, and still emits the empty document when `.github` is missing

## 6. Tests and guardrails

- [ ] 6.1 Create `tests/integ_audit.rs`: clean lock, branch-entry lock, empty lock, missing
      token
- [ ] 6.2 Add the lock-is-the-only-source test — workflows referencing an unlocked action
      produce no finding
- [ ] 6.3 Add `audit` to the two command-module lists in `tests/code_health.rs`
- [ ] 6.4 Add the code-health assertion that no `src/audit/` file imports the workflow
      scanner or parsed-workflow types
- [ ] 6.5 Confirm `gx lint` still performs no network I/O and its tests are unchanged

## 7. Documentation and verification

- [ ] 7.1 Add `gx audit` to the README command list, noting the `GITHUB_TOKEN` requirement
- [ ] 7.2 Judge whether `docs/demo.tape` needs a change and record the reasoning
- [ ] 7.3 Run `mise run test` and `mise run integ`; both must pass
- [ ] 7.4 Verify directory budgets: `src/audit/` leaves at least 3 free slots,
      `src/infra/github/` stays within 8
