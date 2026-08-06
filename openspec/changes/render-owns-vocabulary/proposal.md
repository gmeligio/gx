## Why

The noun "action" is written into lint rule messages and into the `Line` output
type rather than chosen at the render boundary, so the vocabulary a user reads has
no single owner. Today that shows as inconsistency: `unpinned` says
`action actions/checkout uses tag reference v4 instead of SHA pin` while
`stale-comment`, `sha-mismatch` and every workflow-security rule say nothing of the
kind — same output surface, two conventions.

It is also the same class of bug the codebase already guards against for a
different field. `format_line_lint_diag_renders_location_once`
(`src/output/lines.rs`) exists because a rule that embeds the workflow path in its
message makes the path print twice — the renderer owns location. The identical
discipline was never applied to the noun, and it now costs something concrete: gx
manages composite action files as well as workflows, and GitLab CI components are
planned, so a hardcoded noun cannot follow the artifact it describes.

## What Changes

- Rename the `action: String` field on `Line::Upgraded`, `Added`, `Removed`,
  `Changed`, `Skipped` to `id: String`. The field carries a coordinate
  (`actions/checkout`), not a noun; the name is what fixes vocabulary at the type
  level. Renderer output is unchanged — the field is interpolated positionally.
- Drop the leading `action ` prefix from the four rule messages that carry it
  (`unpinned`, `sha-mismatch`, `stale-comment`, `unsynced-manifest`), bringing them
  in line with the majority convention already followed by the other eleven rules.
  This is the one deliberate user-visible text change; see Impact.
- Add `format_line_lint_diag_message_carries_no_noun` next to the existing
  location-printed-once test, asserting a rule message does not open with a
  vocabulary noun, so a future rule cannot reintroduce one.
- Leave summary text (`All actions up to date`, `N actions discovered`) exactly as
  it reads today. Those sentences are composed inside a `CommandReport::render`,
  which *is* the render boundary — the noun is already owned there, and no rename
  is needed to keep it owned.

**No `--json` contract change.** `UpgradeEntry.action` and the `SkippedEntry`
serialization keep their field names; only the internal `Line` type is renamed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `command-output`: adds a guardrail requirement — user-facing vocabulary is chosen
  at the render boundary, not embedded upstream. This sits beside the existing
  "Commands do not print directly" guardrail and is load-bearing for the same
  reason: violating it produces the double-render / inconsistent-phrasing class of
  bug the spec already exists to prevent. Per the relevance gate, the field rename
  alone would be *internal refactoring, skip spec*; the guardrail is what earns the
  spec, because it constrains how user value is delivered and because one
  user-visible message is being normalized under it.

## Impact

**User-visible text — one deliberate change.** Four `gx lint` messages lose a
leading `action ` prefix:

```
- action actions/checkout uses tag reference v4 instead of SHA pin
+ actions/checkout uses tag reference v4 instead of SHA pin
```

The issue's acceptance criterion is byte-identical output, and it also names the
inconsistency ("some messages say 'action', some do not") as the problem to solve.
Those two cannot both hold: the prefix is either kept (inconsistency stays, and the
noun stays baked into the rule) or dropped (four messages change). Dropping it is
the reading that resolves the issue rather than restating it, and it costs little —
the identifier `actions/checkout` immediately follows, the rule name (`unpinned:`)
already prefixes the line, and no test or documented workflow greps for the word.
Everything else — symbols, colors, spacing, column widths, summary lines, exit
codes, `--json` — stays byte-for-byte identical.

**Code.**
- `src/output/lines.rs` — field rename, renderer arms, new guard test.
- `src/upgrade/report.rs`, `src/tidy/report.rs` — construct `Line` with the new
  field name. `UpgradeEntry`'s serialized `action` field is untouched.
- `src/lint/unpinned.rs`, `sha_mismatch.rs`, `stale_comment.rs`,
  `unsynced_manifest.rs` — drop the noun from message construction.
- `src/init/report.rs`, `src/lint/report.rs` — unchanged.

**Not touched.** `--json` output, exit codes, `Diagnostic`'s shape, the ignore-target
config keys (`action = "..."` in `gx.toml` is a config surface, not output).
