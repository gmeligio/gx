## Context

`audit-command-shell` (#129) landed `gx audit`: the command, the token guard, the `--json`
contract, the `rule_ids!`-generated `CheckName`, the `AuditTarget` lock projection, and a
GraphQL advisory seam. It shipped exactly one check, `mutable-ref`, which needs no network.

This change adds the second check and, with it, the first check that actually issues a
request. Three other checks (#130, #131, and the advisory consumer) are being developed in
parallel against the same base, which constrains the shape of this work more than its size
would suggest: anything this change does to a shared file is a merge conflict for someone
else.

The relevant existing pieces:

- `Registry` (`src/infra/github/registry.rs`) owns the blocking `reqwest` client, the token
  header, and `check_status`, which classifies a non-2xx response into `Error::RateLimited`
  / `Unauthorized` / `NotFound` / `ApiError`.
- `advisory.rs` is the precedent for a seam: a trait checks depend on, a real adapter
  wrapping `Registry`, and a `#[cfg(test)]` fake in the same file.
- `ActionId::base_repo()` (`src/domain/action/identity.rs:17`) takes the first two
  slash-separated segments and returns a `Repository`.

## Goals / Non-Goals

**Goals:**

- Report actions published from archived repositories at `warn`, with the last-push date
  and migration guidance in the message.
- Resolve subpath actions to their base repository before the lookup.
- Make the check unit-testable with no network.
- Keep adding a check to `CheckName` a one-line edit, so parallel work does not conflict.
- Distinguish "not archived" from "could not check", and never let the second read as the
  first.

**Non-Goals:**

- Configuration (`[audit.rules]`, per-check ignores, `--audit-level`). Deferred by #129 for
  the whole command; nothing here changes that.
- Caching or deduplicating requests across lock entries. See Decision 6.
- Reporting `disabled` repositories, or reporting staleness on its own. See Decision 7.
- Concurrency. Requests are sequential, matching every other network path in gx today.

## Decisions

### Decision 1: Derive the repository from `ActionId::base_repo()`, do not re-add `AuditTarget.repository`

`AuditTarget` carries `id`, `version`, `sha`, `ref_type`. #129 dropped the lock row's
`repository` field as unused. The obvious move is to add it back, since this check needs a
repository. **Rejected.**

The lock's `repository` field is not a trustworthy input here. `src/infra/lock/format.rs:87`
deserializes it verbatim from `gx.lock` on disk:

```rust
repository: Repository::from(commit_data.repository.as_str()),
```

`gx.lock` is a file in the user's repository. Nothing validates that its `repository` field
agrees with the action id it sits beside — a hand-edit, a bad merge, or a bug in an older gx
that wrote the field differently all produce a lock where they disagree. Every *writer* in
production code does set it from `base_repo()` (`registry.rs:189`, `registry.rs:267`,
`format.rs:239`), so the brief's claim that `Commit.repository` is always `id.base_repo()`
holds for locks gx wrote — but it is an invariant of the writers, not of the type, and
audit reads locks it did not write.

`base_repo()` derives the same value from `id`, which is the key the lock is indexed by and
therefore cannot disagree with itself. Deriving is both safer and less code: no field, no
adapter line, no new borrow in the struct.

So `AuditTarget` is **unchanged by this change**. That is the strongest possible outcome for
the parallel-work constraint — `target.rs` sees no structural edit at all.

*Verified during implementation:* one production call site does write a non-`base_repo`
value — `src/upgrade/command.rs:180` — but it is inside `mod tests`, a test helper. The
production writers are unanimous. The argument above does not depend on that call site.

### Decision 2: A new REST seam, separate from the advisory seam

`advisory.rs` exists and is a seam. Reusing it would mean widening `AdvisoryQuery` with a
`repo_metadata` method, forcing the advisory-consuming check (#131) to depend on a method it
never calls and this check to depend on `advisories`, which it never calls.

Instead: `src/infra/github/repo_meta.rs` with the same three-part shape.

```rust
pub struct RepoMeta {
    pub archived: bool,
    pub pushed_at: String,
}

pub trait RepoMetadata {
    fn metadata(&self, repo: &str) -> Result<RepoMeta, Error>;
}

pub struct RestRepoMetadata { registry: Registry }
```

Normalizing at the integration edge — a two-field struct rather than raw JSON — means check
logic never touches a `serde_json::Value`, matching how `Advisory` is normalized.

**The fake lives in `repo_meta.rs` under `#[cfg(test)]`, not in a `repo_meta_fake.rs` of its
own.** The reason is the directory budget: `src/infra/github/` holds **7** files against a limit of 8
(`tests/code_health.rs`, `folder_file_count_budget`). One new file lands it at exactly 8 —
passing, with zero headroom. Two would break the budget outright and force it to be raised.
The fake is small and is the same module's test double, so co-locating it costs nothing in
clarity.

`Registry::get_json<T>` **does** exist (`registry.rs:141`) and is the right tool here: it
sends an authenticated GET, classifies a non-2xx response through `check_status` *before*
parsing, and decodes the body — exactly the three steps this adapter needs. The adapter uses
it rather than hand-rolling the sequence. (`advisory.rs` does not, because GraphQL is POST
and reports failures in a 200 body.)

The directory's actual contents are `advisory.rs`, `dates.rs`, `mod.rs`, `registry.rs`,
`resolve.rs`, `responses.rs`, `tags.rs`. There is no `advisory_fake.rs` — the advisory fake
already lives inside `advisory.rs` under `#[cfg(test)]`, for this same budget reason. So
co-locating this fake follows the directory's precedent rather than departing from it.

`pushed_at` is kept as `String`, not parsed into a date type. It is never compared or
sorted — only displayed — so a real date type would buy nothing behavioral. `CommitDate`
elsewhere in gx is likewise a newtype over `String`.

It is not displayed *verbatim*, though: see Decision 8 for the truncation, and for what is
shown when the value is not the shape GitHub documents. The truncation is a display choice
made at the point of formatting, which is why it does not argue for parsing at the seam.

### Decision 3: The check owns its errors as findings, not as a command-level `Err`

A failed lookup could abort the whole command with `Err`, the way a missing token does.
**Rejected.** The token guard is a precondition — it is knowable before any work starts, and
without it *nothing* can be checked. A failed lookup for one action out of thirty is
different: twenty-nine results are valid, and discarding them would make one flaky request
erase a run's entire output.

So a failed lookup becomes an **error-level `Finding`** attributed to the same
`archived-action` check. This satisfies both halves of the requirement: the run exits 1
(because an error-level finding is present), so it cannot be mistaken for clean, and the
other entries still report.

The alternative of a `warn`-level failure finding was rejected: `warn` does not change the
exit code, so a CI job would stay green while having checked nothing.

### Decision 4: `archived-action` — one-line registration

```rust
crate::rule_ids! {
    CheckName {
        MutableRef => "mutable-ref",
        ArchivedAction => "archived-action",
    }
}
```

`Display`, `FromStr`, `as_str`, `ALL`, `Serialize`, and `Deserialize` all follow from the
macro. Nothing else in `check_name.rs` changes, so #130 and #131 adding their own line
conflicts only on adjacent lines, which git resolves.

### Decision 5: One new file, `src/audit/archived.rs`

`src/audit/` is at 4 of its 8-file budget. This takes one slot, leaving 3 for #130 and #131.

The check does *not* go into `target.rs` alongside `mutable_ref`. `mutable_ref` is a pure
function of an `AuditTarget`; this check takes a `&dyn RepoMetadata` and has its own error
handling and message construction. Putting it in `target.rs` would also guarantee a conflict
with any other check that touched the same file, and would push `target.rs` toward the
440-line logic budget.

### Decision 6: One request per lock entry, no cache

Two entries for the same base repository (`github/codeql-action/init` and
`github/codeql-action/analyze`) issue two identical requests. A cache would remove one.

Not doing it: a lock with duplicate base repos is uncommon, `GET /repos/{owner}/{repo}` is
one of the cheapest calls in GitHub's API against a 5000/hour authenticated budget, and a
cache is state that has to be threaded through the check and tested. If a real lock proves
this slow, a `HashMap` inside the adapter is a contained follow-up that changes no
interface.

### Decision 7: Report `archived` only — not `disabled`, and not staleness on its own

The REST response also carries `disabled`. It is not checked: a disabled repository returns
404 for most operations, so the failed-lookup path already covers it, and GitHub's docs are
thin on when it is set for public repos.

Staleness alone is deliberately not a finding. Plenty of good actions are simply finished
and do not need commits. `pushed_at` is reported only *alongside* `archived`, where it
grades a risk that has already been established, rather than being a risk claim of its own.

### Decision 8: The finding text

```
actions/setup-ruby is published from an archived repository (last pushed 2021-04-14);
it will receive no security fixes, so migrate to a maintained alternative
```

Three parts, each earning its place: **what is true** (archived), **how stale**
(`pushed_at`), and **why it matters plus what to do** (no fixes → migrate). Per the
constraint that a `warn` a user cannot act on becomes noise they filter out, the message
must not stop at "is archived" — that names a fact without naming a consequence, and the
reader's fair response is "so what?". The word "migrate" is chosen over "upgrade" precisely
because upgrading is the thing that cannot work here.

`pushed_at` arrives as an ISO-8601 timestamp (`2021-04-14T18:22:31Z`). Only the date part is
shown; the time of day is noise at the granularity of "how many years frozen".

Truncation is by splitting on `T` and taking the first segment, **falling back to the whole
string when there is no `T`**. Nothing validates the shape before this point — `pushed_at`
is an unparsed `String` (Decision 2) — so a value GitHub returns in some other form must not
panic or silently blank the date. The fallback shows whatever arrived, which is strictly
more informative than dropping it and keeps a malformed timestamp from turning a real
archived-repository finding into a misleading one. This is a display-level fallback, not an
error path: the finding is still correct and still actionable, only its date reads oddly.

The failure finding is a different message on the same check:

```
could not determine whether actions/checkout is archived: GitHub API rate limit exceeded
for https://api.github.com/repos/actions/checkout
```

It leads with the uncertainty rather than the cause, so a user skimming a report cannot read
it as a verdict about the action. The trailing reason is `Error`'s own `Display`, so
`Registry::check_status`'s existing classification — `NotFound` vs `RateLimited` vs
`Unauthorized` — reaches the user without this check restating it. That is what makes a
rename ("not found") distinguishable from a blip ("rate limit exceeded") despite the shared
opening clause.

## Automated Test Strategy

**Level:** unit tests, in-file under `#[cfg(test)]` at the bottom, matching every other
module in the tree. No new test infrastructure and no new integration test binary.

**Critical path — what must not be allowed to break:**

1. **An archived repository produces a finding** carrying the action name, the date, and
   migration guidance. Asserted on message *content*, not just finding count, since the
   message is the entire user-facing deliverable.
2. **A non-archived repository produces no finding.** This is an absence assertion and is
   the easiest test in the change to write vacuously — if the fake returned `archived:
   false` for everything, or the check never ran, it would pass identically. It is made
   non-vacuous by asserting on the fake's recorded call list: the test proves the lookup
   *happened* and returned `archived: false`, rather than proving nothing happened. Its
   mutation is inverting the `if` in the check; if the test does not fail under that
   mutation, the test is wrong.
3. **A subpath action is looked up by its base repository.** Asserted by inspecting the
   repository slug the fake recorded, which is the only way to observe it — the finding
   alone cannot distinguish the two.
4. **A failed lookup yields an error-level finding, not silence**, and does not suppress
   findings for other entries.
5. **The seam parses a real response shape.** The adapter's JSON decoding is tested against
   a literal payload in GitHub's documented format, so a field rename breaks a test rather
   than production.

**Mutation testing:** each scenario above is verified by breaking the corresponding
behavior, confirming the test fails, and restoring. Results are reported with the change.

**Not tested:** the live GitHub API. `mise run e2e` already exercises real network paths for
the resolution code; adding a live archived-repo assertion would make the suite depend on
`actions/setup-ruby` staying archived, which is outside the project's control.

## Observability

**How a finding surfaces:** through the existing `gx audit` paths — an `OutputLine::LintDiag`
in human output carrying `archived-action` and the message, and an entry in the `findings`
array under `--json` with `check`, `level`, and `message`. This change adds no new output
line type and no new JSON key, so existing consumers parse it without modification.

**Error paths, and whether a failure can be silent:**

| Failure | Surfaces as | Silent? |
|---|---|---|
| Repo archived | `warn` finding | No |
| Request fails / rejected / rate limited | `error` finding, exit 1 | No |
| Response unparseable | `error` finding, exit 1 | No |
| 404 (repo renamed, deleted, or private) | `error` finding, exit 1 | No |
| No token | Existing `Error::MissingToken`, exit non-zero, no report | No |

The design's central observability property is that **no failure produces a clean result.**
Every path above is either a finding or a hard error; there is no branch on which the check
returns `None` because something went wrong. `None` is returned in exactly one situation —
the lookup succeeded and reported `archived: false` — which is a positive statement.

The 404 row deserves note: a renamed repository 404s, and the user sees an error finding
rather than a redirect being followed. That is the correct default for a security command
(a repository that is no longer where the lock says it is *is* worth a user's attention),
though it means a rename produces a noisier result than strictly necessary.

**Progress:** the existing `on_progress("Auditing locked actions...")` callback is
unchanged. Per-action progress messages were considered and rejected — under `--json` they
are suppressed anyway, and in human mode the spinner already indicates liveness.

## Risks / Trade-offs

- **A large lock makes `gx audit` slow** → One sequential request per entry. Rate limit is
  not the constraint (5000/hour authenticated); wall-clock is. Accepted for now: matches the
  existing behavior of `gx upgrade`, and Decision 6's cache is available if a real lock
  proves it a problem.
- **A renamed repository is reported as an error, not followed** → See Observability. Judged
  the safer default, and the message names the URL so the cause is diagnosable.
- **A transient network blip turns a green CI run red** → By construction (Decision 3), and
  the alternative is a green run that checked nothing. The message distinguishes a failed
  lookup from a verdict so the user knows to retry rather than to investigate the action.
- **This check is not a vulnerability check** → The point the proposal and the spec both
  address head-on. Mitigated by the finding text explaining *why* archived matters, and by
  the absence of a config surface making removal cheap if it proves unwelcome.
- **Parallel-change conflict** → Minimized structurally: one line in `check_name.rs`, one
  new file, one line in `mod.rs`'s check pipeline, and *no* change to `target.rs`.

## Migration Plan

Not applicable — additive. A user who upgrades gets one additional check. Because it is
`warn`-level, no build that passed before fails after, unless a metadata lookup fails
(error-level), which is a genuine new failure mode and is intended to be visible.

Rollback is deleting `src/audit/archived.rs`, its two `mod.rs` lines, and one line in
`check_name.rs`. No persisted state and no file format changes.

## Open Questions

None blocking. The one deferred judgment is Decision 7's exclusion of `disabled`, which can
be added as a second boolean on `RepoMeta` without touching the trait or the check's shape
if it turns out to matter.
