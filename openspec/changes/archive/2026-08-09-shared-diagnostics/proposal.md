## Why

`src/lint/` is a command module that also owns the general diagnostics framework.
Seven of its eight concerns — the diagnostic record, severity levels, ignore
matching, error/warn counting, exit-code mapping, summary pluralization, and rule
identity — are not lint-specific. The forthcoming `gx audit` command (#129) needs
all of them and would otherwise duplicate every one.

Two concrete defects follow from the current shape:

1. **Rule identity is three hand-synchronized lists.** `RuleName`
   (`src/lint/rule.rs`) is an enum, a hand-written `Display`, and a hand-written
   `FromStr` — three lists that must be kept in step by hand. This is not a
   hypothetical: the `[lint.rules]` config surface accepts a rule name only if all
   three agree, so any drift is a user-visible bug. The same hand-sync already
   produced drift in our own spec, whose "All valid rule names accepted" scenario
   enumerates 10 of the 13 rules that actually exist.

2. **A root-level module reaches forward into a command module.** `src/config.rs`
   and `src/infra/manifest/convert.rs` both import `crate::lint::RuleName`. Config
   and infra are lower-level than any command; the dependency arrow points the
   wrong way, and it points that way only because rule identity is filed under the
   command that happens to use it today.

Doing this now is what unblocks the queue: `src/lint/` is at 8/8 files, so #128
(lint --json) and #109 (a new lint rule) cannot land until it has a free slot, and
#130/#131/#132 each add one audit check — under three hand-synced lists in one
file they conflict by construction and cannot run in parallel.

## What Changes

- Introduce a shared diagnostics home holding the genuinely general vocabulary:
  the diagnostic record, its builder, the ignore matchers, and the report
  counting / exit-code / summary logic — parameterized over a rule-identity type
  so `lint::RuleName` and a future `audit::CheckName` are two instantiations of
  one vocabulary rather than two copies of it.
- **Collapse the three synchronized lists into one.** The enum's `serde`
  `rename_all = "kebab-case"` derive is already the single authoritative
  name list; `Display` and `FromStr` are re-expressed in terms of it instead of
  restating it. Adding a rule becomes a one-line enum edit, so the three queued
  audit-check issues touch one line each and stop conflicting.
- Repoint `src/config.rs` and `src/infra/manifest/convert.rs` at the shared home,
  removing the config → command dependency inversion.
- Free at least one file slot in `src/lint/`, unblocking #128 and #109.
- Correct **both** places the name drift occurred in the `lint-command` spec — the
  "All valid rule names accepted" scenario (10 → 13 names) and the "Zero-config
  runs all rules at defaults" default set (10 → 13 rules) — and make both enforced
  by tests driven off the single definition rather than restated by hand.

Not in scope: no `src/audit/` module and no audit check — #129 does that later.
User-visible output is unchanged and byte-identical.

## Capabilities

### New Capabilities

None. This introduces no new user-facing capability.

### Modified Capabilities

- `lint-command`: two requirements change.
  - "Configure rule severity" gains a guarantee that the set of accepted rule
    names is derived from one definition rather than maintained by hand, so a rule
    can never be accepted in one direction and rejected in the other. Its "All
    valid rule names accepted" scenario is corrected from the 10 names it drifted
    to, to the 13 that exist.
  - "Zero-config runs all rules at defaults" gains a guarantee that every
    implemented rule appears in the documented default set, and its list is
    corrected from 10 to 13 (`dangling-reference` = error, `invalid-expression` =
    error, `run-shellcheck` = warn were missing — the spec already contradicted
    itself, since its own `run-shellcheck` requirement states "default level:
    warn" inline).

Per the relevance gate, the module move and file reshuffle on their own are
"internal refactoring with no user-visible change" and are deliberately left
unspecced. What earns a spec is the round-trip guarantee on the `[lint.rules]`
config surface: today a name is accepted only if three hand-maintained lists
agree, and the drift that gate is meant to catch has already occurred in this very
spec. That is a change in what the system guarantees to the user, not merely how
it is built.

## Impact

- **Code**: new shared diagnostics module; `src/lint/rule.rs`, `src/lint/report.rs`,
  `src/lint/mod.rs`, `src/lint/command.rs` and the individual rule files repoint at
  it; `src/config.rs` and `src/infra/manifest/convert.rs` drop their
  `crate::lint::` imports.
- **Public API**: `gx::lint::{Diagnostic, RuleName, Context, Rule}` are re-exported
  from their new home so `tests/integ_lint.rs` and `tests/common/setup.rs` keep
  compiling unchanged.
- **Dependencies**: none added. The single name list is obtained from the `serde`
  derive already on the enum.
- **Output**: unchanged. `mise run integ` asserts on output text and is the proof.
- **Unblocks**: #128, #109 (need a `src/lint/` slot); #129 and its #130/#131/#132
  checks (need the shared vocabulary and the one-line rule addition).
