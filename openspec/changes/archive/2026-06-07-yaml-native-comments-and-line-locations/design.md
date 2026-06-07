## Context

gx reads workflow YAML through `serde-saphyr` into a structural `Parsed` model, and writes
changes back through surgical regex replacement that preserves bytes (comments, quoting,
indentation). These are two deliberately separate paths: the **reader** builds a model for
rules to inspect; the **writer** never serializes that model back out.

Until the 0.0.27 bump, saphyr's reader dropped inline comments and exposed no source spans.
gx compensated with a second, regex-based pass over the raw text to recover the `# v4`
version comment attached to each `uses:` line. That pass is the subject of this change.

`serde-saphyr` 0.0.27 provides:
- `Commented<T>(pub T, pub String)` — deserializes a scalar into its value plus its trailing
  inline comment (scalar-only; complex values drop the comment). Verified against gx's
  `# v4` comment shape with a throwaway probe.
- `Spanned<T>` — wraps a value with `.value` and `.referenced` location info exposing
  `.line()` / `.column()`. Verified to report 1-based line numbers for scalar values.

These compose: `Spanned<Commented<String>>` yields value, comment, and line in one parse.

## Goals / Non-Goals

**Goals:**
- One parse pass produces the value, its inline comment, and its source line.
- Delete the regex comment-scraper and the duplicate-comment overwrite bug it carries.
- `gx lint` prints `file:line` for violations that map to a single workflow line.

**Non-Goals:**
- Changing the write path. `workflow_update.rs` stays regex-based and byte-preserving.
- Serializing the parse model back to YAML (gx never does this; this change does not start).
- Column-level precision in diagnostic output (line is enough for the UX win; column is
  available from the span if a later change wants it).
- Surfacing `shellcheck` findings at their true source line. That builds *on* the span
  plumbing this change lands, but is a separate change with its own spec delta.
- New lint rules, new config, or any new command.

## Decisions

### Decision 1: `Step.uses` becomes comment-and-span aware

Change `Step.uses` from `Option<String>` to `Option<Spanned<Commented<String>>>` (PR 2's
end state; PR 1 introduces `Option<Commented<String>>` and PR 2 wraps it in `Spanned`).

**Why**: The comment and the line both belong to the *same scalar*. Capturing them where the
scalar is parsed keeps them correct by construction — no re-association by string matching,
no second pass that can disagree with the first.

**Alternatives considered**:
- *Keep `String`, add a parallel `Spanned` map*: reintroduces the re-join-by-equality
  fragility this change is trying to delete.
- *Wrap every scalar in the model*: unnecessary. Only the scalars whose comment or line gx
  actually consumes need wrapping. Start with `uses`; widen only when a rule needs it.

### Decision 2: Delete the regex comment-scraper outright

`USES_WITH_COMMENT_RE`, the `for line in content.lines()` comment loop, the
`HashMap<uses-text, comment>`, and the `comments.get(uses)` lookup in `scanner.rs` are
removed. The comment comes from `step.uses`'s parsed `Commented` value.

**Why**: It is dead weight once the comment rides on the value. Keeping it would mean two
sources of truth for the same comment that can disagree.

**Bug fixed**: the `HashMap` keyed on uses-text means two steps with identical `uses:` but
different pinned comments collapse to one entry (last write wins). Reading the comment from
each step's own parsed value gives each step its own comment. No separate fix needed — the
refactor removes the shared map that caused it.

### Decision 3: `Diagnostic` gains `line: Option<u32>`, not a required field

Add `line: Option<u32>` to `Diagnostic` with a `with_line` builder, mirroring the existing
`with_workflow` / `with_job` / `with_step` builders.

**Why**: Not every diagnostic has a meaningful single line. Manifest-level and whole-file
rules (`unsynced-manifest`, missing-permissions on an absent block) have no one line to
point at. `Option` lets those rules omit it and lets the formatter fall back to the current
`file:` rendering.

### Decision 4: Output renders `file:line` only when the line is present

`Line::LintDiag` rendering appends `:line` after the workflow path when `line` is `Some`,
producing `path:line: rule: message`. When `None`, output is byte-identical to today.

**Why**: Zero regression for diagnostics that can't carry a line, and a clickable
`file:line` for those that can.

### Decision 5: Two PRs, one spec

PR 1 (`Commented`) is internal — it changes no spec-visible behavior, so it ships under this
spec with no scenario of its own beyond "output unchanged." PR 2 (`Spanned` + line) delivers
the modified `Clean diagnostic output` requirement. Splitting the implementation keeps each
diff small and independently reviewable; merging the spec keeps the rationale in one place
because the two share the `Step.uses` type change.

## Risks / Trade-offs

- **`Commented` is scalar-only.** If a `uses:` value were ever a non-scalar (it never is —
  it's always a string), the comment would silently drop. gx's `uses:` is always a scalar
  string, so this is not a practical risk; noted for the record.
- **Span line is the value's line, not the comment's.** For `uses: x@sha # v4`, the line is
  the `uses:` line — which is exactly what the user wants to jump to. No trade-off in
  practice.
- **Type churn in the parse model.** Changing `Step.uses`'s type touches every reader of it.
  The blast radius is small (the scanner and any rule that reads `uses`), and the compiler
  enforces that every reader is updated. PR 1 absorbs most of this churn so PR 2 is small.

## Open Questions

<!-- none -->
