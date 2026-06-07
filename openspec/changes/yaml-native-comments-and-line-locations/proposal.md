## Why

The `serde-saphyr` 0.0.27 upgrade (merged in #102) exposes two parser capabilities gx
does not yet use: `Commented<T>` captures a scalar's trailing inline comment *during* the
structural parse, and `Spanned<T>` carries the scalar's source line/column. gx currently
works around the absence of both:

- **Comments**: gx parses each workflow twice — once structurally via saphyr (which drops
  comments), then again with a hand-rolled regex loop that re-scrapes `# v4`-style version
  comments line-by-line into a `HashMap` keyed on the `uses:` text, then re-joins by string
  equality. This duplicate parse is fragile and carries a **latent correctness bug**: two
  steps that use the *same* action with *different* pinned comments collapse to one map
  entry — the second silently overwrites the first.

- **Locations**: lint diagnostics name the workflow, job, and step but **not the line**.
  A user running `gx lint` on a repo with many workflows is told *what* is wrong and in
  *which file*, but must then eyeball the file to find *where*. This is the weakest part
  of the lint UX, and it blocks the natural follow-on of surfacing `shellcheck` findings
  at their true source line.

Adopting the native parser features retires the regex comment-scraper (less code, one
parse, bug fixed) and threads real source lines into diagnostics (better UX). One concern,
delivered in two PRs.

## What Changes

- **Workflow read model carries native comments and spans.** `Step.uses` (and any other
  scalar whose inline comment or line gx needs) becomes a comment- and span-aware type from
  saphyr instead of a bare `String`. The structural parse becomes the single source of both
  the value and its `# version` comment.
- **The regex comment-scraper is removed.** `USES_WITH_COMMENT_RE`, the per-line
  `HashMap<uses-text, comment>` loop in `scanner.rs`, and the "re-join by string equality"
  lookup go away. The duplicate-comment overwrite bug goes away with them.
- **Lint diagnostics gain an optional source line.** `Diagnostic` carries an optional
  `line`, populated from the parsed span of the offending scalar. Diagnostic output renders
  the line when present.
- **Lint output shows `file:line`.** A located violation prints `path:line:` so the user can
  jump straight to it; violations with no meaningful single line (whole-file or
  manifest-level) render as before.

### Write path is explicitly out of scope

`workflow_update.rs` continues to rewrite `uses:` refs via surgical, byte-preserving regex
replacement. gx never serializes its parse model back to YAML — doing so would reflow
formatting and reorder/strip comments. saphyr's *reader* gains nothing for the writer, so
the write path is untouched.

## Capabilities

### New Capabilities

<!-- none — no new user-facing command or config surface -->

### Modified Capabilities

- `lint-command`: lint diagnostic output gains a source line (`file:line`) for violations
  that map to a single workflow line.

## Impact

- `src/domain/workflow_parsed/mod.rs`: `Step.uses` type changes from `Option<String>` to a
  comment-/span-aware type; `Parsed::from_yaml` unchanged in shape.
- `src/infra/workflow_scan/scanner.rs`: delete the regex comment-scraper and the comment
  `HashMap`; read the comment from the parsed value; populate the action's source line.
- `src/lint/rule.rs`: add `line: Option<u32>` to `Diagnostic` plus a builder method.
- `src/output/lines.rs`: render the line in `LintDiag` output when present.
- `src/domain/workflow_actions.rs`: `Location` / `Located` may carry the line so rules can
  pass it into diagnostics.
- No changes to `workflow_update.rs`, the manifest/lock write paths, or any command other
  than `lint`'s output formatting.

## Delivery

Implemented as two sequential PRs against a single spec:

1. **`Commented<T>` refactor** — internal-only, no spec-visible behavior change. Retires the
   regex scraper and fixes the duplicate-comment bug. Lowest risk, highest maintenance ROI.
2. **`Spanned<T>` + line in diagnostics** — the user-visible `file:line` change this spec
   delta describes. Builds on the same `Step.uses` type PR 1 introduces.
