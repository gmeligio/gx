## 1. The tag-resolution seam

- [ ] 1.1 Add `src/infra/github/tag_ref.rs` defining a `TagResolver` trait with one method
      resolving `(action, tag)` to a commit SHA, returning `Result<CommitSha, Error>`.
- [ ] 1.2 Implement the real adapter in the same file, building the
      `GET /repos/{owner}/{repo}/git/ref/tags/{tag}` URL and delegating to the existing
      `Registry::fetch_ref_commit`, which already dereferences annotated tags. Do NOT add
      lines to `resolve.rs` — it is at 438 of its 440-logic-line budget.
- [ ] 1.3 Handle subpath actions (`github/codeql-action/upload-sarif`) by taking the first two
      path segments, matching what `resolve_ref` does.
- [ ] 1.4 Add a `#[cfg(test)]` fake implementation returning canned commits or a canned
      failure, and recording which `(action, tag)` lookups it was asked for.
- [ ] 1.5 Export the trait, the real adapter, and the fake from `src/infra/github/mod.rs`.
- [ ] 1.6 Confirm `src/infra/github/` is still within its 8-file budget.

## 2. The check

- [ ] 2.1 Add `TagMoved => "tag-moved"` to the `rule_ids!` list in `src/audit/check_name.rs`.
      This must be a one-line edit — no hand-written `Display`, `FromStr`, or serde.
- [ ] 2.2 Add `src/audit/tag_moved.rs` holding the check: skip entries whose `ref_type` is not
      `Tag` or `Release`, resolve the rest through the seam, compare against the locked SHA.
- [ ] 2.3 Produce an error-level finding on mismatch, naming the action, version label, locked
      SHA, and current SHA, using the wording from the design ("the tag was moved").
- [ ] 2.4 Produce an error-level finding when the lookup fails or the tag is absent upstream,
      stating the tag could not be verified and why.
- [ ] 2.5 Confirm `src/audit/` holds at most 5 `.rs` files, leaving slots for #130 and #132.

## 3. Wiring

- [ ] 3.1 Give `Audit` a constructor taking a `TagResolver`, so tests inject the fake and
      `main.rs` injects the real adapter built from the token the command already requires.
- [ ] 3.2 Thread the resolver into `collect_findings` and run the check there — one line
      alongside `mutable_ref`, per the parallel-development contract.
- [ ] 3.3 Update `src/main.rs`'s `run_audit` to construct `Audit` with the real adapter.
- [ ] 3.4 Verify `src/audit/mod.rs` stays within its 360-logic-line budget.

## 4. Tests

- [ ] 4.1 Unit: an unmoved lightweight tag produces no finding.
- [ ] 4.2 Unit: a moved tag produces one error-level finding carrying both SHAs.
- [ ] 4.3 Unit: an unmoved annotated tag produces no finding — the false-positive path that
      matters most.
- [ ] 4.4 Unit: a release-resolved entry is checked like a tag.
- [ ] 4.5 Unit: branch, bare-commit, and `ref_type: None` entries produce no finding AND
      trigger no lookup, asserted against the fake's recorded calls. Asserting only "no
      finding" would be vacuous.
- [ ] 4.6 Unit: a failed lookup produces an error-level finding.
- [ ] 4.7 Unit: with one failing and one moved entry, both produce findings.
- [ ] 4.8 Adapter: assert the URL built for a tag lookup, including the subpath case.
- [ ] 4.9 Integration in `tests/integ_audit.rs`: a moved tag exits 1 and its `--json` finding
      carries `"check": "tag-moved"` and `"level": "error"`.
- [ ] 4.10 Fix the existing `integ_audit.rs` tests so they issue no network requests: inject
      the fake into `Audit`, and change any subprocess (`run_gx`) fixture using a tag pin to a
      branch or commit pin, since a spawned binary cannot take the fake.

## 5. Verification

- [ ] 5.1 Run each mutation and record the result: invert the eligibility check so branches are
      looked up; make the adapter return the tag object's SHA instead of dereferencing; make a
      failed lookup produce no finding. Each must turn a test red. Restore after each.
- [ ] 5.2 `mise run test` passes.
- [ ] 5.3 `mise run integ` passes.
- [ ] 5.4 Run the built binary against this repository's own `.github/gx.lock` with a real
      token. All entries are tag- or release-resolved, so every one exercises a live lookup;
      all must report clean.
- [ ] 5.5 Confirm no numeric budget in `tests/code_health.rs` was raised.
