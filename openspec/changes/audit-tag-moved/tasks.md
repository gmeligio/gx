## 1. The tag-resolution seam

- [x] 1.1 Add `src/infra/github/tag_ref.rs` defining a `TagResolver` trait with one method
      resolving `(action, tag)` to a commit SHA, returning `Result<CommitSha, Error>`.
- [x] 1.2 Implement the real adapter in the same file, building the
      `GET /repos/{owner}/{repo}/git/ref/tags/{tag}` URL and delegating to the existing
      `Registry::fetch_ref_commit`, which already dereferences annotated tags. Keep this out
      of `resolve.rs`: that file owns the tag/branch/commit fallback chain, and "resolve
      exactly this tag" is a different question.
- [x] 1.3 Handle subpath actions (`github/codeql-action/upload-sarif`) by taking the first two
      path segments, matching what `resolve_ref` does.
- [x] 1.4 Add a fake implementation returning canned commits or a canned failure, and
      recording which `(action, tag)` lookups it was asked for. It must NOT be `#[cfg(test)]`:
      `tests/integ_audit.rs` compiles against the non-test lib, so a gated fake is invisible
      there and tasks 4.9/4.10 cannot inject it. Follow the precedent that already solves
      this — `pub mod testutil` in `src/domain/resolution.rs`, exported ungated — rather than
      `FakeAdvisories`/`FakeChecker`, which are gated and unreachable from `tests/`.
- [x] 1.5 Export the trait, the real adapter, and the fake from `src/infra/github/mod.rs`.
- [x] 1.6 Confirm `src/infra/github/` is within its 8-file budget. It holds 7 files today, so
      `tag_ref.rs` lands at exactly 8/8 — no slack, and adding another file there forces a
      split.

## 2. The check

- [x] 2.1 Add BOTH `TagMoved => "tag-moved"` and `TagUnverified => "tag-unverified"` to the
      `rule_ids!` list in `src/audit/check_name.rs`. Two lines, no hand-written `Display`,
      `FromStr`, or serde. Shipping only `tag-moved` silently breaks the requirement that a
      tag gx could not resolve is reported under its own name.
- [x] 2.2 Add `src/audit/tag_moved.rs` holding the check: skip entries whose `ref_type` is not
      `Tag` or `Release`, resolve the rest through the seam, compare against the locked SHA.
- [x] 2.3 Produce an error-level finding on mismatch, naming the action, version label, locked
      SHA, and current SHA, using the wording from the design ("the tag was moved").
- [x] 2.4 Produce an error-level finding when the lookup fails or the tag is absent upstream,
      stating the tag could not be verified and why.
- [x] 2.5 Confirm `src/audit/` is within its 8-file budget. It holds 4 today, so `tag_moved.rs`
      takes it to 5, leaving room for #130 and #132.

## 3. Wiring

- [x] 3.1 Give `Audit` a constructor taking a `TagResolver`, so tests inject the fake and
      `main.rs` injects the real adapter built from the token the command already requires.
- [x] 3.2 Thread the resolver into `collect_findings` and run the check there — one line
      alongside `mutable_ref`, per the parallel-development contract.
- [x] 3.3 Update `src/main.rs`'s `run_audit` to construct `Audit` with the real adapter.
      `main.rs` is at 435 of its 440-logic-line budget, so this edit must stay small.
- [x] 3.4 Verify `src/audit/mod.rs` stays within the 360-line budget the mod.rs check applies
      (it counts non-structural lines; the file sits near 39 today, so there is ample room).

## 4. Tests

- [x] 4.1 Unit: an unmoved lightweight tag produces no finding.
- [x] 4.2 Unit: a moved tag produces one error-level finding carrying both SHAs.
- [x] 4.3 Unit: an unmoved annotated tag produces no finding — the false-positive path that
      matters most.
- [x] 4.4 Unit: a release-resolved entry is checked like a tag.
- [x] 4.5 Unit: branch, bare-commit, and `ref_type: None` entries produce no finding AND
      trigger no lookup, asserted against the fake's recorded calls. Asserting only "no
      finding" would be vacuous.
- [x] 4.5b Unit: an entry pairing `ref_type = "tag"` with a SHA-shaped version label produces
      no finding AND triggers no lookup. This is the guard whose absence yields the false
      accusation, so it needs the same recorded-calls assertion as 4.5.
- [x] 4.6 Unit: a failed lookup produces an error-level finding.
- [x] 4.7 Unit: with one failing and one moved entry, both produce findings.
- [x] 4.8 Adapter: assert the URL built for a tag lookup, including the subpath case, and
      that it delegates to `fetch_ref_commit` rather than carrying a second dereference.
- [x] 4.9 Integration in `tests/integ_audit.rs`: a moved tag exits 1 and its `--json` finding
      carries `"check": "tag-moved"` and `"level": "error"`. Assert `error_count` is 1 too —
      these are the capability's first error-level findings, so a finding that renders as
      `error` without incrementing the count would satisfy every scenario and still break the
      published contract. Cover the two-name split here as well: one moved and one unreachable
      entry yield different `check` values.
- [x] 4.10 Fix the existing `integ_audit.rs` tests so they issue no network requests: inject
      the fake into `Audit`, and change any subprocess (`run_gx`) fixture using a tag pin to a
      branch or commit pin, since a spawned binary cannot take the fake.

## 5. Verification

- [ ] 5.1 Run each mutation and record the result: invert the eligibility check so branches are
      looked up; make the adapter return the tag object's SHA instead of dereferencing; make a
      failed lookup produce no finding. Each must turn a test red. Restore after each.
- [x] 5.2 `mise run test` passes.
- [x] 5.3 `mise run integ` passes.
- [ ] 5.4 Run the built binary against this repository's own `.github/gx.lock` with a real
      token. First confirm every entry is still tag- or release-resolved (8 entries: 7 tag,
      1 release at the time of writing), so every one exercises a live lookup; then confirm
      all report clean.
- [x] 5.5 Confirm no numeric budget in `tests/code_health.rs` was raised.
