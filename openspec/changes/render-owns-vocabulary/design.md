## Context

`Line` (`src/output/lines.rs`) is gx's single rendering boundary — the
`command-output` spec already forbids commands from printing directly. Two things
route around it for the noun "action":

1. `Line::Upgraded / Added / Removed / Changed / Skipped` each carry a field named
   `action: String`. The value is a coordinate (`actions/checkout`); the *name*
   pins the vocabulary at the type level, so widening gx to composite action files
   (already shipped) or GitLab components (planned) means the type says the wrong
   word.
2. Four lint rules prepend the literal `action ` to their message before it reaches
   the renderer. Eleven other rules do not. The renderer cannot undo it — the
   message is opaque `String` by then.

The codebase already enforces exactly this discipline for a sibling field:
`format_line_lint_diag_renders_location_once` exists because a rule that embeds the
workflow path makes it print twice. The noun never got the same treatment.

## Goals / Non-Goals

**Goals:**
- The render boundary is the only place a user-facing noun is chosen.
- The `Line` field name states what it carries (an identifier), not what kind of
  thing it names.
- A guard test makes reintroducing an embedded noun fail CI, in the shape of the
  existing location test.
- Every output byte outside the four normalized lint messages is unchanged.

**Non-Goals:**
- No terminology configuration, noun registry, `Kind` enum, or i18n layer. Nothing
  today needs to *vary* the noun — it needs a single owner. A registry would be
  speculative and would have to be threaded through `Diagnostic`, both report
  types, and the renderer for zero present benefit.
- No change to the `--json` contract.
- No rewording of summary lines.

## Decisions

### Rename `action` → `id`, do not introduce a `Kind` enum

`id: String` says the field carries an identifier and stops the type from asserting
what kind of thing it identifies. The alternative — `Line::Upgraded { kind: Kind,
id: String }` with the renderer interpolating `kind` — was rejected: no current
output line prints a noun at all (`↑ actions/checkout  v3 → v4`), so `Kind` would
be a field every producer must set and no renderer arm would read. That is a
vocabulary framework, not single ownership.

`id` over `identifier`/`coordinate`/`name`: shortest, and `ActionId` /
`Spec.id` / `Diagnostic`'s neighbours already use it, so it reads as the house term.

### Normalize the four rule messages by dropping the `action ` prefix

The issue asks for byte-identical output *and* names inconsistency ("some messages
say 'action', some do not") as the defect. Both cannot hold. Options:

- **Keep the prefix, rename only the field.** Byte-identical, but the noun stays
  inside rule messages — the larger half of the issue's root cause is untouched,
  and the guard test cannot be written because current rules would fail it.
- **Add the prefix to all fifteen rules.** Consistent and byte-non-identical in the
  other direction, and it moves *more* vocabulary upstream. Backwards.
- **Drop the prefix from the four.** Chosen. It resolves the inconsistency, empties
  the noun out of rule messages, and makes the guard test enforceable.

Cost is genuinely small: the identifier follows immediately, the rule name already
prefixes the rendered line (`✗ ci.yml:7: unpinned: actions/checkout uses tag
reference v4 instead of SHA pin`), and no test, doc, or README example greps for
the word. Recorded here because it is the one place this change is visible to a
user.

### Leave summary lines alone

`All actions up to date` and `N actions discovered` are composed inside
`CommandReport::render` — that *is* the render boundary. The noun is already owned
where it should be. Renaming nothing there keeps those lines byte-identical and
keeps the change to its actual scope.

## Risks / Trade-offs

- **[Four lint messages change text; a user grepping for `action ` in lint output
  breaks.]** → Accepted deliberately and recorded above. The identifier and rule
  name are the stable things to grep, both unchanged. Called out in the change
  report so it can reach release notes.
- **[A `--json` field is renamed by accident, breaking a public contract.]** →
  `UpgradeEntry.action` and `SkippedEntry.action` are `Serialize` fields and stay
  untouched; only the internal `Line` type is renamed. `to_json_*` tests assert the
  JSON keys directly and would fail if this slipped.
- **[The guard test is written so loosely it never fires, or so tightly it blocks a
  legitimate message.]** → Assert on the specific leading-noun shape (message does
  not start with `action `/`workflow `/`component `) rather than substring-anywhere,
  so a message legitimately containing the word mid-sentence still passes.

## Automated Test Strategy

Unit-level, at the boundary being changed — no new infrastructure.

- **Guard (new), `src/output/lines.rs`:** `format_line_lint_diag_message_carries_no_noun`,
  sibling to `format_line_lint_diag_renders_location_once`. Feeds a `LintDiag`
  whose message is what a rule actually produces and asserts no kind-noun prefix.
- **Guard (new), rule-side:** extend the four rules' tests to assert the produced
  `Diagnostic.message` does not begin with `action `, mirroring the existing
  `message_does_not_embed_workflow_path` test in `unpinned.rs`. The renderer-side
  test guards the shape; the rule-side test names the culprit when it fails.
- **Byte-identical evidence (existing, must stay green unmodified):** the
  `format_line_*` tests in `lines.rs` pin rendered strings for every variant;
  `render_upgrade_*`, `render_tidy_*`, `render_init_*`, `render_lint_*` pin exact
  summary text (`"2 upgraded · 1 file"`, `"1 removed · 2 added · 1 upgraded · 2
  files"`, `"All actions up to date"`, `"2 actions discovered · manifest created"`).
  These are the proof: a field rename that altered rendering would fail them.
- **`--json` contract (existing, unmodified):** `to_json_uses_resolved_versions_and_compare`,
  `to_json_omits_compare_when_absent`, `to_json_up_to_date_has_empty_upgrades`
  assert JSON keys directly.
- **Critical path:** `mise run test` plus `mise run integ`. `integ_lint.rs` asserts
  on `Diagnostic` values rather than rendered text, so it confirms rule *behavior*
  is unchanged while the four messages are reworded.

## Observability

This change has no runtime error paths — it is a rename plus a string edit, all
resolved at compile time. A mistake surfaces as a build failure (field rename) or a
failing assertion on exact text (rendering or JSON), never as a silent
wrong-output-at-runtime, because every rendered line and every JSON key under
change is pinned by an existing test asserting an exact string.

The one failure that *could* be silent is the guard test being written too loosely
to catch a future embedded noun. Mitigated by asserting the specific prefix shape
and by keeping the rule-side assertions, so two independent tests must both be
wrong for a regression to land.

## Migration Plan

None. No data, config, or persisted format changes. Rollback is `git revert` — no
state to unwind.

## Open Questions

None. The one judgment call (dropping the `action ` prefix, against a literal
reading of "byte-identical") is decided above and surfaced in the change report.
