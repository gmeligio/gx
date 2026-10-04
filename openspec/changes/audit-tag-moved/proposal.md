## Why

Every major GitHub Actions supply-chain compromise has the same signature: an attacker
repoints existing tags at a malicious commit. `tj-actions/changed-files` (CVE-2025-30066)
repointed v1–v45.0.7 and hit 23,000+ repositories; `aquasecurity/trivy-action`
(GHSA-69fq-xp46-6x23) had 75 of its 76 tags repointed; `codfish/semantic-release-action` and
`actions-cool/issues-helper` the same. A user who pinned to a SHA is not compromised by this,
but they have no way to learn it happened — and a user whose workflow still names the tag is.

Detecting a moved tag requires knowing what the tag pointed to **when it was pinned**.
`gx.lock` is that baseline, and gx is the only tool in this ecosystem that keeps one. zizmor's
`stale-action-refs`, the closest existing audit, asks a different question — "was this SHA ever
tagged?" — and runs offline with no record of the prior target, so a tag moved after you pinned
it is invisible to it.

## What Changes

- Add a `tag-moved` audit check: for each `gx.lock` entry resolved to a tag or release,
  resolve that tag live against GitHub and compare the commit it points at now with the SHA
  the lock recorded. A mismatch is reported at **error** severity, naming both SHAs.
- Annotated tags are dereferenced to their target commit before comparing, so a legitimate
  annotated tag never produces a finding.
- A subpath action (`github/codeql-action/upload-sarif`) resolves against its repository root,
  since tags belong to the repository — otherwise a healthy dependency would 404 and be
  reported as unverifiable.
- Entries resolved to a branch or a bare commit are skipped: neither has a tag whose movement
  could be measured, and reporting on them would be a false accusation.
- Introduce a substitutable seam for live tag resolution, mirroring the advisory seam from
  `audit-command-shell`, so the check is testable offline and deterministically.
- A failed lookup is an error-level finding, never silence — the first case where `gx audit`
  exits non-zero because it *could not check*, which `audit-command-shell` deliberately left
  to the first consuming check to specify.

## Capabilities

### New Capabilities

None. This extends the `audit-command` capability introduced by `audit-command-shell`.

### Modified Capabilities

- `audit-command`: adds the `tag-moved` check requirement, a `tag-unverified` requirement for
  entries gx could not resolve, and a requirement that live tag resolution goes through a
  substitutable seam whose failures surface as findings rather than as silence. Both are user-facing: they add a new finding a user can see, a new reason the
  command exits non-zero, and a new guarantee about what a clean run means.

**Relevance gate:** this adds user-facing behavior — a new check with its own name, severity,
message, and exit-code consequence — so it requires a spec.

## Impact

- `src/audit/check_name.rs`: two entries added to the `rule_ids!` list — `tag-moved` and
  `tag-unverified`.
- `src/audit/`: one new file holding the check and its tag-resolution seam. The directory is
  at 4 of its 8-file budget; this takes one slot.
- `src/audit/target.rs`: `AuditTarget` gains nothing; `ref_type` already distinguishes tag,
  release, branch, and commit, which is exactly the skip logic this check needs.
- `src/audit/mod.rs`: the check is wired into `collect_findings`, which gains a tag-resolver
  argument. `Audit::run` constructs the real resolver from the token it already requires.
- `src/infra/github/`: reuses the existing annotated-tag dereferencing rather than
  reimplementing it. No new dependency. The new `tag_ref.rs` brings the directory from 7 to
  8 files, exactly its budget.
- Network cost: one request per tag-or-release lock entry, plus one more per annotated tag.
