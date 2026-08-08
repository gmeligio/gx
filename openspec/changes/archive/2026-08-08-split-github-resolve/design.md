## Context

`src/infra/github/` currently holds four files:

| file | total | logic | after (total) |
| --- | ---: | ---: | ---: |
| `mod.rs` | 10 | 10 | 14 |
| `registry.rs` | 258 | 258 | 294 |
| `resolve.rs` | 544 | 438 | 150 |
| `responses.rs` | 86 | 86 | 86 |
| `tags.rs` | — | — | 237 |
| `dates.rs` | — | — | 55 |

The "after" column is filled in at implementation time (task 5.5). The largest
post-split file is `registry.rs` at 294 total, leaving ~250 lines of headroom
against the 550 budget.

`resolve.rs` is 2 lines under the 440 logic-line budget and 6 under the 550
total-line budget. `tests/code_health.rs` also caps a directory at 8 `.rs` files,
so `src/infra/github/` has room for 4 more.

The module's public surface is two names — `mod.rs` reexports only
`{Error, Registry}`. Everything `resolve.rs` defines is either `pub` on
`Registry` (reachable only through that reexport) or `pub(super)`. Moving code
between files inside the module is therefore invisible outside it.

Constraints that bind this design:

- No budget in `tests/code_health.rs` may be raised. That file is edited
  concurrently by other agents; two agents each bumping a number merge cleanly
  and the larger wins, silently. Restructuring is the only permitted fix.
- `#[cfg(test)]` blocks must be last in a file, with no top-level public item
  after them.
- Clippy runs pedantic with private-item and field docs required, and every
  `#[expect(...)]` must be fulfilled — an `#[expect]` that no longer applies is
  itself an error.
- `src/domain/`, `src/lint/`, and `src/audit/` are owned by other agents and must
  not be touched.

## Goals / Non-Goals

**Goals**

- Every file in `src/infra/github/` lands well under budget, with enough headroom
  that #137, #145, and #141 can each add code without re-splitting.
- Each new file names one job.
- Zero behavior change: identical endpoints, identical call order, identical
  error mapping.

**Non-Goals**

- No retry, backoff, or rate-limit handling (#137 owns that).
- No registry trait or GitLab abstraction (#145 owns that). This change must not
  guess at what that abstraction wants.
- No change to `Error` or its variants (#141 owns that).
- No change to any public signature, and no new public item.

## Decisions

### Decision 1: Extract the repeated request/response sequence into one helper

Eight methods in `resolve.rs` repeat the same nine-line shape:

```
authenticated_get(url).send().map_err(Error::Request { operation, url })?
if !response.status().is_success() { return Err(check_status(...)) }
response.json().map_err(Error::ParseResponse { url })?
```

The only things that vary are the URL, the `operation` string, and the
deserialization target. Collapsing this into

```rust
fn get_json<T: DeserializeOwned>(&self, url: &str, operation: &'static str) -> Result<T, Error>
```

removes roughly 100 lines and is the single largest contributor to getting under
budget.

*Why this and not something larger:* the guidance for this change is explicit
that a shared-plumbing seam should be taken only if it falls out naturally. It
does — this is deduplication of literally repeated code, not invented
indirection. What is deliberately *not* built: no trait, no request-builder type,
no injectable transport. #137 gets one place to add retry and #145 gets one place
to parameterize the host, but neither is designed for here.

*Placement:* `registry.rs`, next to `authenticated_get` and `check_status`, which
it composes and which already live there. It stays `pub(super)`. `registry.rs`
grows by roughly 12 lines to about 270 — well inside budget — and no new file is
needed for it. It needs a doc comment: `missing_docs_in_private_items` is denied.

*Two call sites are exempt and must NOT be routed through the helper.* There are
nine `authenticated_get` sites, not eight; the two that do not match the shape
above are exempt for reasons that are behavioral, not stylistic:

- `dereference_tag` returns `Option` and uses `.ok()?` rather than `map_err`. Its
  failures are deliberately silent — a failed dereference yields a missing tag,
  not an error. Routing it through `get_json` would turn that `None` into an
  `Err` and change what `get_tags_for_sha` returns.
- `get_version_tags` calls `parse_next_link(response.headers())` *before*
  `response.json()`, and `json()` consumes the response. Headers cannot be read
  after `get_json` swallows it, so pagination keeps its own explicit request path.

*Alternative rejected:* a `request.rs` file holding the helper. It would be a
~15-line file whose contents belong beside `authenticated_get`, splitting HTTP
plumbing across two files for no gain.

### Decision 2: Three files, split by job

| file | contents |
| --- | --- |
| `resolve.rs` | `resolve_ref` and the tag → release → branch → commit fallback chain: `fetch_ref_commit`, `fetch_commit_sha`, `GITHUB_API_BASE` |
| `tags.rs` | tag enumeration: `get_tags_for_sha`, `dereference_tag`, `get_version_tags`, `parse_next_link`, `filter_refs_by_sha` |
| `dates.rs` | `fetch_commit_date`, `fetch_release_date`, `fetch_tag_date` |

This is the grouping already latent in the file — the three clusters share no
helpers with each other beyond `get_json` and `GITHUB_API_BASE`, and the existing
`#[cfg(test)]` block splits cleanly along the same lines (resolve tests stay with
`resolve.rs`, `filter_refs_by_sha` tests move to `tags.rs`).

*Why keep the name `resolve.rs`:* it keeps naming the fallback chain, which is
what the name always meant. Renaming it would churn `mod.rs` and the module's
mental map for no benefit.

*Alternative rejected — split by HTTP verb or by response type:* produces files
named after mechanics rather than jobs, and scatters the fallback chain.

*Alternative rejected — two files instead of three:* folding `dates.rs` into
`tags.rs` groups "things that return an `Option<String>` date" with "things that
return tag names", which is not one job.

### Decision 3: `GITHUB_API_BASE` stays in `resolve.rs`, imported by the others

The constant is needed by all three files. Options were: duplicate it (rejected —
two sources of truth), move it to `registry.rs` (defensible, but `registry.rs`
owns the client and auth, not URL layout), or keep it in `resolve.rs` and have
`tags.rs` and `dates.rs` import it via `super::resolve::GITHUB_API_BASE`.

The third is chosen as the smallest move. It requires widening the constant from
private to `pub(super)`, which is a visibility change on a private item — not a
public-surface change. If #145 later needs a per-host base URL, it will move the
constant then, with a reason to.

### Decision 4: The `multiple_inherent_impl` expectation

`resolve.rs` carries
`#[expect(clippy::multiple_inherent_impl, reason = "resolution logic is in a separate file for clarity")]`
because it adds a second `impl Registry` block. After the split there will be
four such blocks (`registry.rs` plus three), so each of the three new/split files
needs the same `#[expect]`. Since clippy requires every `#[expect]` to be
fulfilled, the attribute must be present on exactly the blocks that trigger the
lint — the first `impl Registry` (in `registry.rs`) must not carry it.

## Automated Test Strategy

No new tests. That is the point: this change is behavior-preserving, so the
existing suite passing **unmodified** is the evidence. Adding tests would weaken
that signal by changing the thing being held fixed.

- **Unit tests** move with the code they cover. The four tests in `resolve.rs`'s
  `#[cfg(test)]` block split: `full_sha_passthrough`,
  `subpath_action_extracts_base_repo`, and `version_resolver_trait` stay with
  `resolve.rs`; the three `filter_refs_by_sha` tests move to `tags.rs`. Test
  bodies are not edited, only relocated, and `mod tests` stays at the bottom of
  each file.
- **Integration:** `mise run integ`.
- **E2E:** `tests/e2e_github.rs` exercises `resolve_ref` and `get_tags_for_sha`
  against the live API, including the annotated-tag dereference path. It needs
  `GITHUB_TOKEN` or it fails with `RateLimited`, so it is not part of the local
  gate; run it when a token is available.
- **Critical path** — the behavior most at risk from a bad move, in order:
  1. `get_version_tags`'s pagination loop. It is the largest block that must be
     transcribed by hand (it is exempt from `get_json`, see Decision 1) and it
     has **no test coverage at all** — no unit test, and `tests/e2e_github.rs`
     covers only `resolve_ref` and `get_tags_for_sha`. An inverted or dropped
     `match next_url { Some(next) => url = next, None => break }` branch would
     silently return only the first page and every existing test would still
     pass. This one cannot be caught by running the suite; it has to be caught
     by reading the diff.
  2. annotated-tag dereferencing (`fetch_ref_commit`'s owner/repo re-extraction
     from the ref URL is fiddly string surgery — it must move verbatim),
  3. the tag → release → branch → commit fallback order in `resolve_ref`,
  4. `operation` strings on `Error::Request`, which appear in user-facing error
     messages and are easy to transpose when routing calls through `get_json`.
- **Gate:** `mise run test` (typecheck, format, lint, size budgets, lockfile,
  unit tests) must pass, with `tests/code_health.rs` unmodified. Its file-count,
  file-size, and logic-line assertions are what verify the split actually
  achieved its purpose.

## Observability

Nothing about how failures surface changes, and that is the requirement rather
than a side effect.

- Every error path stays a typed `Error` variant returned to `registry.rs`'s
  `VersionRegistry` impl, which maps it to `ResolutionError` for the user. Both
  mappings are untouched.
- `Error::Request` carries `operation` and `url`; `Error::ParseResponse` carries
  `url`. Routing calls through `get_json` must preserve each call site's existing
  `operation` string exactly — these strings are the only thing distinguishing
  otherwise-identical error messages, so a silent swap here would degrade
  diagnostics without failing any test.
- **Can a failure be silent?** Two places already swallow errors and still do:
  `dereference_tag` returns `Option` and drops errors (a failed dereference
  yields a missing tag, not an error), and `resolve_ref` discards the tag-lookup
  error before falling through to the branch attempt. This change preserves both
  as-is; it neither introduces new silent paths nor fixes these. #141 is the
  place to revisit them.
- The genuine risk is a *silently successful* refactor that changes which
  endpoint is hit or in what order, since a wrong-but-working call chain can pass
  unit tests. Mitigated by diff review of call order per method, and — for
  `resolve_ref` and `get_tags_for_sha` only — by the e2e suite. `get_version_tags`
  has no test covering it at any level, so for that method diff review is the
  *only* mitigation.

## Risks / Trade-offs

- **A method is transcribed with the wrong `operation` string or URL template.**
  → These are string literals with no compile-time check. Review the diff
  method-by-method against the pre-split file rather than reading the result in
  isolation.
- **`get_json` changes behavior subtly.** The current code calls
  `check_status(&response, url)` *before* `json()`, so a non-2xx body is never
  parsed. → The helper must keep that order. A helper that parsed first would
  turn `NotFound` into `ParseResponse` and break `resolve_ref`'s fallback chain,
  which depends on tag lookup failing cleanly.
- **`fetch_ref_commit` does two sequential requests** (ref, then tag
  dereference) and cannot become a single `get_json` call. → It uses `get_json`
  twice; the inline URL-reconstruction logic between them moves verbatim.
- **Another agent lands a conflicting edit in `src/infra/github/`.** → The
  8-file directory budget is shared. The split adds 2 files, leaving 2 of
  headroom; if a concurrent change consumes them, the file-count assertion fails
  loudly rather than silently.

## Migration Plan

Not applicable — no data, no persisted format, no public API. Rollback is
`git revert` of a single commit.

## Open Questions

None. The one judgement call — whether to extract shared HTTP plumbing at all —
is settled in Decision 1: take it because it is present as literal duplication,
and take nothing beyond it.
