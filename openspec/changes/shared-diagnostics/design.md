## Context

`src/lint/` is a command module that also owns gx's general diagnostics
framework. Of the eight concerns living there, seven are not lint-specific:

| Concern | Location today | Lint-specific? |
| --- | --- | --- |
| `Diagnostic` record + builders | `lint/rule.rs:79-139` | no — only its `rule: RuleName` field |
| Ignore matching (3 matchers) | `lint/rule.rs:214-316` | no |
| Error/warn counting | `lint/report.rs:19-34` | no |
| `Level` → exit code | `lint/report.rs:78-80` | no |
| Summary pluralization | `lint/report.rs:60-72` | no |
| `Context` (manifest/lock/workflows) | `lint/rule.rs:142-163` | no |
| `RuleName` identity | `lint/rule.rs:18-75` | **the identity is, its machinery is not** |
| `Rule` trait + the 13 rules | `lint/*.rs` | yes |

`gx audit` (#129) needs the first seven and would otherwise copy them.

Two facts found while verifying the brief, both of which shaped this design:

1. **The hand-synced lists are four, not three.** Beyond the enum, `Display`, and
   `FromStr`, the `#[serde(rename_all = "kebab-case")]` derive is a fourth
   name-producing mechanism.
2. **`FromStr` has no production callers.** Grep across `src/` finds it used only
   by its own unit tests. The `[lint.rules]` config surface — the thing that
   actually parses rule names from `gx.toml` — goes through serde's `Deserialize`,
   not `FromStr`. `Display` has exactly one production caller, `report.rs:52`,
   which renders the rule name into user-visible output.

So one of the four lists is dead weight, and the live config surface is driven by
a derive that already holds the canonical list.

Evidence the hand-sync fails in practice: the `lint-command` spec's "All valid
rule names accepted" scenario enumerates 10 rule names; 13 exist. The three
newest (`dangling-reference`, `invalid-expression`, `run-shellcheck`) were added
to the code without the list being updated.

## Goals / Non-Goals

**Goals:**

- One authoritative list per rule-identity type. Adding a rule is a one-line edit.
- A shared diagnostics home usable by both `lint` and a future `audit`.
- `src/config.rs` and `src/infra/manifest/convert.rs` stop importing from a
  command module.
- At least one free file slot in `src/lint/`.
- Byte-identical user-visible output.

**Non-Goals:**

- No `src/audit/` module, no audit check (#129 owns that).
- No general-purpose diagnostics framework beyond what lint and audit need.
- No change to rule behavior, severity defaults, ignore semantics, or messages.
- No new external dependency.

## Decisions

### D1: A `rule_ids!` declarative macro owns the single list

The macro takes `Variant => "kebab-name"` pairs once and generates the enum,
`as_str`, `Display`, `FromStr`, `ALL`, and the serde impls from that one list.

```rust
rule_ids!(RuleName { ShaMismatch => "sha-mismatch", ... });
```

Adding a rule is one line inside the macro invocation. #130/#131/#132 each add
one line to a *different* enum (`audit::CheckName`), so they cannot conflict.

**Alternatives considered:**

- *`strum` derive crate* — solves it, but adds a proc-macro dependency and build
  time for one enum (two after audit). The repo currently has 18 direct
  dependencies and no proc-macro helper of this kind. Rejected as
  disproportionate; a 25-line `macro_rules!` needs no dependency.
- *Keep the enum, derive only `Display` from serde* (via `serde_plain` or a
  `Serialize`-to-string round trip) — leaves the enum and the serde rename as two
  lists and makes `Display` pay a serialization cost for what is a static string.
  Rejected.
- *Hand-written `as_str` + derive `Display`/`FromStr` from it, keeping
  `rename_all`* — reduces four lists to two (enum-with-`as_str`, and the serde
  rename). Better, but still lets the serde name and the `Display` name drift,
  which is precisely the drift that matters because one feeds config parsing and
  the other feeds output. Rejected in favor of D2.

### D2: serde is driven by the same list, not by `rename_all`

`rename_all = "kebab-case"` is dropped. The macro emits `Serialize`/`Deserialize`
implementations that delegate to `as_str` / `FromStr`, so the config surface and
the rendered output provably read from one list.

This is the load-bearing half of the change: it is what makes "accepted in config"
and "printed in output" the same set by construction rather than by discipline.
It also means the corrected 13-name spec scenario can be enforced by a test that
iterates `RuleName::ALL` instead of restating names.

**Byte-compatibility check**: `rename_all = "kebab-case"` maps `ShaMismatch` →
`sha-mismatch`; every existing `Display` arm already produces exactly that string.
The two lists agree today for all 13 variants, so collapsing them changes no
name. This is verified by a test asserting each variant's serde form equals its
`as_str`, and by the existing `gx.toml` fixtures in `tests/`.

**Map-key round-trip**: `RuleName` is a `BTreeMap` key in
`src/infra/manifest/convert.rs:69`, which is the manifest *write* path as well as
the read path. Serializing as a map key is a distinct serde constraint from
serializing as a value: the impl must emit a plain string (`serialize_str`), not a
unit variant, or TOML key emission breaks. The macro's `Serialize` therefore
delegates to `as_str` via `serialize_str`, and a test round-trips a populated
`[lint.rules]` table through serialize→deserialize to confirm the write path is
byte-stable.

One deliberate behavior preservation: serde's derived error for an unknown map key
(`unknown variant ...`) differs in wording from `FromStr`'s
(`unrecognized rule name: ...`). Because the manual `Deserialize` now routes
through `FromStr`, the message users see for a typo'd rule name changes wording.
The spec requires only "an error identifying the unrecognized rule name", which
both satisfy; the new message names the offending key and is at least as clear.
This is called out in the delta spec rather than left implicit.

### D3: The shared home is `src/diagnostic/`, a new top-level module

`src/diagnostic/` holds `record.rs` (the `Diagnostic` record + builders + ignore
matchers) and `report.rs` (counting, exit code, summary). `mod.rs` re-exports.

Placing it at `src/` top level — a sibling of `config`, `domain`, `output` — is
what fixes the inversion: `config.rs` importing `crate::diagnostic::` points
*down* at a shared vocabulary module, not *forward* at a command.

**Alternatives considered:**

- *`src/domain/diagnostic.rs`* — `src/domain/` is at 7/8 and the module is about
  reporting, not the action/lock domain. Also would put it under a module
  `config.rs` does not otherwise depend on. Rejected.
- *`src/output/diagnostic.rs`* — `output/` owns rendering, and #142 just
  established that rendering is a boundary the producers stay behind. A
  diagnostic is produced upstream of rendering. Rejected.
- *Leave it in `lint/` and have audit import from lint* — preserves the exact
  inversion this change exists to remove. Rejected.

### D4: Rule identity stays a closed, enumerable type

`Diagnostic` names `RuleName` concretely. The decision that carries weight is that
the identity remains a **closed set that can be enumerated**: `RuleName::ALL` is
what makes the two guard tests in this change writable at all:

- `every_reported_rule_name_is_accepted_in_config` builds a `[lint.rules]` table
  from `ALL` and parses it, proving the config surface accepts every name gx can
  print.
- `every_rule_has_a_documented_default_level` pairs `ALL` against the documented
  default set, so a rule cannot run at an undocumented default.

Both derive their expectations from the definition rather than restating a list,
which is the property that makes them catch drift instead of re-encoding it.

**Alternative considered — `String` identity.** Simpler signature, and it would
drop the parameter entirely. Rejected on present-day grounds: a `String` rule id
cannot be enumerated, so `ALL` disappears and both guard tests above become
unwritable. It also erases the closed-set guarantee that makes an unrecognized
rule name a parse error rather than a silently-ignored key. That is a concrete
loss in this branch, not a hypothetical one.

**Alternative considered — a `Diagnostic<Id>` type parameter,** so `gx audit`
(#129) could later alias `Diagnostic<CheckName>`. Rejected: it has exactly one
instantiation today and no bounds anywhere, so it is generality bought for a
caller that does not exist — the speculative shape the brief forbids. It also
forces a manual `Default` impl on `Report`, since a derive would demand
`Id: Default`. Adding the parameter when audit lands is mechanical, and audit may
well share `RuleName` outright rather than need a second identity type.

### D5: `Context` stays in `lint/`

Despite being listed as generic-shaped, `Context` carries `workflows_full:
&[ParsedWorkflow]` and `action_set`, which exist to serve lint's rules. Audit's
needs are unknown, and the brief forbids speculative generality. Moving it would
be guessing. It stays in `lint/` and #129 moves it if and when it needs it.

### D6: File layout and the `src/lint/` slot count

Before: 8 direct `.rs` files (at the 8-file budget, FULL).

`rule.rs` and `report.rs` leave. `rule.rs`'s residue keeps the name `rule.rs`: what
remains — the `RuleName` identity, the `Rule` trait, the rule-running `Context`,
and the per-rule runner wrappers — is all *about a lint rule*, so the existing
name still describes the file honestly and no call site churns. (An `identity.rs`
holding a trait, a context, and runners would be a junk drawer wearing a specific
name; `types.rs` is banned by the generic-filename rule.) `lint/mod.rs` absorbs
the re-exports — it is 25 lines of pure re-export today, far from the 360-logic
`mod.rs` budget, whose current max of 354 belongs to `lint/run_shellcheck/mod.rs`,
a file this change does not touch.

After: 7 direct `.rs` files (`command.rs`, `mod.rs`, `rule.rs`,
`sha_mismatch.rs`, `stale_comment.rs`, `unpinned.rs`, `unsynced_manifest.rs`)
— **one free slot**, satisfying the hard requirement, with a second available by
folding `unsynced_manifest.rs` into the aggregate phase later if #128 and #109
both need room.

`src/diagnostic/` starts at 3 files (`mod.rs`, `record.rs`, `report.rs`), well
inside budget.

No budget number in `tests/code_health.rs` is raised. Both new files sit far below
the 440-logic-line limit, since the material being moved is ~200 logic lines
total and is being split across two files.

## Automated Test Strategy

**Level:** unit tests for the vocabulary, existing integration tests as the
output-invariance proof. No new test infrastructure.

**Critical path — the three things that could actually break:**

1. **Name drift between config and output.** A new unit test in
   `src/diagnostic/` (or `lint/identity.rs`) iterates `RuleName::ALL` and asserts,
   per variant, that the serde-serialized form, `as_str`, `Display`, and the
   `FromStr` round trip all agree. This is the test that makes D2's guarantee
   real, and it is impossible to satisfy by restating a list because it derives
   its expectations from `ALL`.
2. **The 13-name config surface.** A test parses a `gx.toml` containing every
   name in `RuleName::ALL` under `[lint.rules]` and asserts all 13 land in the
   map — replacing the drifted hand-written 10-name list with a generated one.
   The previously untested "unrecognized rule name" scenario gains a test
   asserting a typo'd key is rejected.
3. **Output byte-identity.** `mise run integ` (`tests/integ_lint.rs`) already
   asserts on rendered diagnostic text and rule names across all rule families.
   It is the regression gate for the move and must pass unchanged — no test
   edits beyond import paths.

**Mutation checks** (run during implementation, not left in the tree): break one
variant's `as_str` string and confirm the agreement test and `integ` both go red;
restore. This is what distinguishes a real guard from a vacuous one.

## Observability

Failures in this change are compile-time or test-time, not runtime — which is the
point of keeping identity a closed enum rather than a string.

- **Unrecognized rule name in `gx.toml`**: surfaces as a parse error naming the
  offending key, propagated through `Error::Manifest` from
  `parse_lint_config`. Loud, non-silent, exits non-zero.
- **Name drift**: cannot occur at runtime — there is one list. If someone
  reintroduces a second list, the agreement test in the critical path fails.
- **A rule silently dropped from output**: prevented by `integ_lint.rs`, which
  asserts per-rule diagnostics by name.

**Can a failure be silent?** The one candidate is the serde rename removal
producing a subtly different name (e.g. `sha_mismatch` vs `sha-mismatch`), which
would silently stop matching users' existing `gx.toml` keys. This is exactly what
the per-variant agreement test and the 13-name parse test exist to catch, and why
D2 records the byte-compatibility argument explicitly rather than assuming it.

## Risks / Trade-offs

- **Removing `rename_all` changes deserialization wiring on the live config
  surface** → Mitigated by the per-variant agreement test, the 13-name parse
  test, and `mise run integ`. The mapping is proven identical for all 13 variants
  before the change lands.
- **The unknown-key error message wording changes** → Accepted and documented in
  the delta spec. The spec's requirement ("an error identifying the unrecognized
  rule name") is still met; only the phrasing differs, and the new phrasing names
  the key.
- **A `macro_rules!` enum is less discoverable than a plain one** (rust-analyzer
  "go to definition" lands on the macro invocation) → Mitigated by keeping the
  macro small, local, and documented, and by the invocation reading as a plain
  list of `Variant => "name"` pairs. Judged a smaller cost than four hand-synced
  lists.
- **Moving `Diagnostic` out of `lint/` touches every rule file's import** →
  Mechanical; `lint::Diagnostic` re-exports it so rule bodies are unchanged. The
  external API (`gx::lint::Diagnostic`) keeps its name, so `tests/` is unaffected.
- **`FromStr` is currently dead code and this change keeps it** → It becomes live:
  `Deserialize` routes through it. That converts dead code into the single parse
  path rather than deleting something audit would re-add.

## Migration Plan

Internal refactor with no persisted-format change; `gx.toml` and `gx.lock` files
are unaffected, so there is no data migration and no user action.

Rollback is `git revert` of the single commit — nothing outside the source tree
changes.
