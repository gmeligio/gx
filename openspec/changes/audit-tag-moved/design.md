## Context

`audit-command-shell` (#129) shipped the `gx audit` command with one offline check
(`mutable-ref`) and a GraphQL advisory seam that no check yet consumes. This change adds
`tag-moved`, the first check that actually reaches the network, and therefore the first that
forces the questions #129 deferred: how a check obtains a network client, and what a run means
when a lookup fails.

The check itself is arithmetic: compare the SHA the lock recorded against the SHA the tag
points at now. Everything difficult is around that comparison — deciding which entries are even
eligible, dereferencing annotated tags so legitimate actions do not trip the check, and making
a failed lookup impossible to mistake for a clean result.

## Goals / Non-Goals

**Goals**
- Detect the movement of a tag between the moment gx pinned it and now.
- Never produce a false positive. A finding here says "you may have been attacked"; being wrong
  is far more costly than missing an edge case.
- Keep adding an audit check a one-line edit in `check_name.rs`, so parallel work does not
  collide.

**Non-Goals**
- Judging *why* a tag moved. gx cannot distinguish a hostile repoint from a maintainer's
  force-push, and should not pretend to. The finding reports the fact; the user investigates.
- Detecting a compromised commit that keeps its tag. That is the advisory seam's job.
- Caching or deduplicating lookups across runs. One request per eligible entry is acceptable at
  the scale of a lock file (this repo's has eight entries) and adding a cache now would be
  speculative.
- Re-walking workflow files. Audit reads the lock; #129 settled this.

## Decisions

### Eligibility is decided by `ref_type`, not by parsing the version string

`AuditTarget` already carries `ref_type: Option<&RefType>`, recorded by the resolver at pin
time. `Tag` and `Release` are eligible; `Branch`, `Commit`, and `None` are skipped.

The alternative — inspecting the version label to guess whether it looks like a tag — was
rejected. It would be a second, weaker notion of what a lock entry is, disagreeing with the
resolver that wrote the entry, and its failure mode is exactly the one that matters: guessing
"tag" for a bare commit produces a lookup for a tag that does not exist, which this design
reports as an error-level finding. A false accusation from a parsing heuristic is the worst
outcome available.

`Release` is eligible because a GitHub release is a tag with release metadata attached — the
same underlying ref, reachable at the same `git/ref/tags/{tag}` endpoint. `RefType::Release`
exists in gx only to record that a release *also* exists; skipping those entries would blind the
check to the most commonly pinned actions.

`None` is skipped rather than attempted. It means the lock predates ref-type recording or was
hand-edited; gx cannot establish the entry names a tag, and a check whose severity is `error`
must not act on an unestablished premise.

**This deliberately diverges from the domain layer, and the divergence must not be "fixed".**
`ResolvedRef::from_stored` (`src/domain/action/resolved.rs`) maps `ref_type: None` to
`Self::Tag`, commented "Tag, Release, or unknown legacy values are all real version tags". That
is right for its callers: resolution and upgrade want a best-effort version to work with, and
guessing wrong there costs a failed lookup. It is wrong here, where guessing wrong costs an
error-level finding telling a user their dependency may have been compromised. Audit reads
`ref_type` directly off `AuditTarget` rather than going through `ResolvedRef` precisely so it
can be stricter. A future reader reconciling the two paths should make the domain stricter, not
audit looser.

### The tag looked up is the lock's version label, guarded against SHA-shaped labels

`AuditTarget.version` is the version label the lock records — the string gx itself wrote when it
pinned the action. For a tag- or release-resolved entry that string *is* the tag name, so it is
what gets resolved.

One case breaks that identity: `ResolvedRef::label` returns the **SHA** when the reference is a
`Commit`, so a lock row's version slot is not universally a tag name. `ref_type` already screens
those out — `Commit` is skipped — but the two fields are written independently and a hand-edited
or legacy row could pair `ref_type = "tag"` with a SHA-shaped label. Left unguarded, that row
would produce a lookup for a tag named `abc123…`, get a 404, and yield an error-level finding
accusing the user of something that never happened.

So eligibility carries a second, cheap condition: skip any entry whose version label is a valid
commit SHA, whatever its `ref_type`. `CommitSha::is_valid` already exists and `resolve_ref` uses
it for the same purpose. This is not version-string *parsing* — the rejected heuristic — it is a
single unambiguous exclusion in the safe direction: the worst case is failing to check an entry
that was never checkable, never a false accusation.

**Consequence for `AuditTarget`:** no new field. The brief anticipated one might be needed;
`ref_type` and `version` together are sufficient, so nothing in `target.rs` changes.

### The seam is a trait over "resolve this tag to a commit", not over HTTP

```rust
pub trait TagResolver {
    fn tag_commit(&self, action: &ActionId, tag: &str) -> Result<CommitSha, Error>;
}
```

One method, taking what the check has and returning what the check compares. Annotated-tag
dereferencing lives *behind* the seam, in the real implementation — so the fake never has to
model tag objects, and the check has no branch for annotated versus lightweight tags at all.
That is deliberate: the annotated-tag path is the most likely source of a false positive, and
the way to make it safe is to give the check no opportunity to get it wrong.

This mirrors `AdvisoryQuery` from #129 and `crate::infra::shellcheck` before it: trait, real
adapter, `#[cfg(test)]` fake.

### The real adapter reuses `Registry::fetch_ref_commit`

`src/infra/github/resolve.rs` already has `fetch_ref_commit(url)`, which GETs a ref endpoint
and, when `object.type == "tag"`, follows `git/tags/{sha}` to the underlying commit. That is
precisely this check's requirement, already written and already exercised by the tidy and
upgrade paths. The adapter builds the `git/ref/tags/{tag}` URL and calls it.

Two constraints shaped this:

1. `fetch_ref_commit` is `pub(super)` — visible only inside `src/infra/github/`. The adapter
   therefore lives in that module, and `src/audit/` depends on the trait.
2. `resolve.rs` is at **438 logic lines against a 440 budget** (`tests/code_health.rs`). It
   cannot absorb new code, and the budget must not be raised. So the trait and its adapter go
   in a new file, `src/infra/github/tag_ref.rs`, which calls `fetch_ref_commit`. The directory
   holds 6 `.rs` files against its 8 budget, so this fits.

Reimplementing the dereference in `src/audit/` was rejected outright: a second copy of the
logic that prevents this check's worst failure mode is the last thing that should be
duplicated.

### A failed lookup is an error-level finding, not a skip and not an abort

Three options were considered.

*Skip the entry.* Rejected. A rate-limited run would silently report clean — the exact
false-assurance `gx audit` refuses to start without a token to avoid. It is the difference
between "I looked and it is fine" and "I could not look", and for a security command those must
never render the same.

*Abort the whole command.* Rejected on user experience. One unreachable action would suppress
findings for every other entry, including real moved tags already detected. A user with a
genuine compromise and one flaky lookup should see the compromise.

*Report it as a finding.* Chosen. The user sees exactly which entries could not be verified and
why, the exit code is non-zero so CI does not go green, and other entries still report.

**Under its own check name, `tag-unverified`.** Reusing `tag-moved` was the first instinct —
same question, answered "unknown" instead of "no" — and it is wrong. #129 fixed `check` as the
machine-readable contract CI consumers filter on, and "an action you depend on was tampered
with" and "GitHub rate-limited us" demand different responses: page a human versus retry the
job. One name would force consumers to regex the prose message to tell them apart, and would
make the two indistinguishable in any test asserting on `check` and `level` alone. Two names
cost one more `rule_ids!` line — the macro exists exactly so that is cheap.

A tag that returns 404 is `tag-unverified` for the same reason a network failure is: gx cannot
distinguish a maintainer cleaning up tags from an attacker deleting evidence, and either way it
has not established where the tag points. Claiming "moved" would assert more than gx knows.

**This diverges from `action-resolution`'s error table**, which classifies rate-limited and
auth-required as recoverable — warn and skip. That classification is right for resolution, where
skipping means "try again later" and the user still has a working lock. It is wrong for audit,
whose entire premise is that a false clean is the worst possible output. The divergence is
deliberate and local to audit; `action-resolution`'s table is unchanged.

### The command owns the resolver; `collect_findings` takes it as a parameter

`Audit::run` already requires a token and already fails without one. It constructs a `Registry`
from that token, wraps it as the real resolver, and passes it to `collect_findings`, which gains
a `&dyn TagResolver` parameter. Tests call `collect_findings` with the fake.

**This changes what the existing integration tests do, and that had to be handled.**
`tests/integ_audit.rs` calls `Audit.run(...)` with the literal token `"token"` and a lock
containing `actions/checkout`. Before this change no check touched the network, so that was
free; after it, `tag_only_lock_is_clean` and friends would issue real, unauthenticated-in-effect
requests to api.github.com — making the suite slow, flaky, network-dependent, and dependent on
whether `actions/checkout@v4.2.1` happens to still point at a fabricated SHA (it does not, so
those tests would newly fail).

The fix is to make the seam injectable at the command level rather than only inside
`collect_findings`: `Audit` becomes `Audit::new(resolver)` — or equivalently gains a constructor
taking the seam — so integration tests construct it with the fake and `main.rs` constructs it
with the real one. Tests that assert on `mutable-ref`, exit codes, and `--json` shape keep
working offline, unchanged in intent.

The subprocess tests in that file (`json_mode_writes_one_document_and_no_progress_output` and
the others using `run_gx`) spawn the real binary and cannot inject a fake. Their fixture locks
must therefore be ones the check does not touch — the branch-pin lock already is, since branches
are skipped, and the tag-pin fixture used by `json_mode_writes_no_local_log_file` must be
changed to a branch or commit pin so the subprocess makes no request. This is recorded as a task
rather than left to discovery.

### Message wording names the attack in the user's terms

```
actions/checkout v4.2.1 now points to a different commit than when it was pinned
(lock a284dc18, now 0e58ed86) — the tag was moved
```

Short SHAs, both present, and the phrase "the tag was moved" — not "SHA mismatch", which is the
existing `sha-mismatch` lint rule's language for an entirely different, offline, repo-internal
condition. A user who sees both should not have to work out which is which.

## Risks / Trade-offs

**A legitimate force-push reads as an attack.** Real risk, accepted. The finding says the tag
moved, which is true; it does not say who moved it or why. Severity `error` is still correct —
an upstream force-push under a pin is something a maintainer must know about and act on, even
when benign.

**One request per eligible entry.** For a lock of a few dozen entries this is a few dozen
requests against an authenticated 5,000/hour limit. Acceptable. If it ever is not, the fix is
batching behind the seam, which the trait already permits without touching the check.

**Annotated tags cost a second request.** Only for annotated tags, only behind the seam, and the
correctness it buys is the whole point.

## Automated Test Strategy

**Unit, against the fake seam** — the critical path, and where the check's judgment is proven:
- an unmoved lightweight tag produces nothing;
- a moved tag produces one error-level finding naming the action, version, and both SHAs;
- **an unmoved annotated tag produces nothing.** The fake models this by returning the
  dereferenced commit, since dereferencing is the adapter's contract, so this test asserts the
  check trusts the seam's answer rather than re-deriving it;
- a release-resolved entry is checked;
- branch, bare-commit, and `ref_type: None` entries are skipped — asserted both by "no finding"
  and by the fake recording **which lookups were requested**, so a skip that happens to produce
  no finding for the wrong reason still fails;
- a failed lookup produces an error-level finding;
- with two entries, one failing and one moved, both report.

The "which lookups were requested" assertion is what makes the skip tests non-vacuous. Asserting
only "no finding" would pass even if the check looked up every branch pin and got lucky.

**Adapter test, no network** — the real adapter's annotated-tag behavior is the one thing the
fake cannot prove. `fetch_ref_commit`'s dereferencing is already covered by the existing
`resolve.rs` tests; the adapter test asserts the URL it builds and that it delegates rather than
re-deriving.

**Integration, `tests/integ_audit.rs`** — a lock on disk through the real parser, the command,
and out to exit code and `--json`, with the fake resolver injected: a moved tag exits 1 and its
JSON finding carries `"check": "tag-moved"` and `"level": "error"`.

**End-to-end, manual and recorded in tasks** — run the built binary against this repository's
own `.github/gx.lock` with a real token. All eight entries are tag- or release-resolved, so
every one exercises a live lookup, and all must come back clean. This is the acceptance
criterion the issue names, and it is also the only test that proves the real adapter's URL and
parsing are right against the actual API.

**Mutation checks, run and recorded** — for each behavior, break it, watch the test fail,
restore. Specifically: invert the eligibility test so branches are checked; make the adapter
return the tag object's SHA instead of dereferencing; make a failed lookup return no finding.
Each must turn a test red.

## Observability

Findings are the surface: check name `tag-moved`, severity `error`, message naming the action,
version, and both SHAs, rendered in the terminal and in `--json` under the contract #129 fixed.

**No failure is silent.** The three ways this check can fail to produce a true answer each have
a visible outcome:
- lookup fails → error-level finding stating the tag could not be verified, and why;
- tag absent upstream → error-level finding naming the action and tag;
- no token → `Audit::run` refuses before any check runs, unchanged from #129.

The remaining silent path is an entry legitimately skipped for its `ref_type` — a branch pin
produces no `tag-moved` finding. That is not a gap: `mutable-ref` already reports branch pins at
warning severity, so such an entry is never invisible to `gx audit` as a whole.

Progress is reported through the existing `on_progress` callback the command already threads, so
a user watching a run of a large lock sees it advancing rather than a stalled spinner.
