## Context

The project currently denies `pedantic`, `perf`, and `nursery` clippy groups. The user added `restriction = "deny"` to explore which restriction lints are valuable. Analysis found ~3,000 violations across ~56 lint rules. After evaluating each lint against real code examples, 35+ lints were selected for adoption and the rest explicitly allowed.

Production code is already unwrap-free. Most violations are mechanical style fixes or test-only safety lint suppressions.

## Goals / Non-Goals

**Goals:**
- Adopt individual restriction lints that catch real bugs or enforce meaningful style
- Bulk-fix all mechanical violations in a single pass
- Add `#[allow]` annotations in test modules for safety lints where test panics are acceptable
- Establish a documented lint configuration contract in `Cargo.toml`

**Non-Goals:**
- Renaming types to remove module name repetitions (separate proposal: `remove-module-name-repetitions`)
- Restructuring types to eliminate `expect` in Result-returning functions (separate proposal: `type-safe-construction`)
- Achieving zero `#[allow]` annotations — targeted suppression in tests is intentional
- Adding `#[non_exhaustive]` to internal types (binary, not library)

## Decisions

### Decision 1: Individual lint adoption over blanket `restriction = "deny"`

**Choice:** Replace `restriction = "deny"` with ~35 individually listed restriction lints.

**Rationale:** Clippy itself warns against blanket restriction denial (`blanket_clippy_restriction_lints`). Many restriction lints are anti-idiomatic (`implicit_return`, `question_mark_used`) or irrelevant for a CLI binary (`missing_inline_in_public_items`, `std_instead_of_core`). Individual selection lets us adopt only lints that provide value.

**Alternative:** Keep blanket deny + allow the ~20 unwanted lints. Rejected because new clippy versions add restriction lints that could break builds unexpectedly.

### Decision 2: Two-tier strategy for safety lints

**Choice:** Deny `unwrap_used`, `expect_used`, `get_unwrap`, `indexing_slicing`, `string_slice`, `panic`, and `assertions_on_result_states` globally, but allow them in test modules.

**Rationale:** Test code legitimately uses `unwrap()` — the test runner catches panics and reports them. Forcing `.expect("reason")` on ~300 test unwraps adds noise without value. Production code is already clean.

**Implementation:** Add `#[allow(clippy::unwrap_used, ...)]` at the top of each `#[cfg(test)] mod tests` block and at the crate level in integration test files.

### Decision 3: `.to_string()` → `.to_owned()` on `&str`

**Choice:** Deny `str_to_string` and convert all ~341 occurrences.

**Rationale:** `.to_string()` on `&str` goes through the `Display` trait's formatting machinery. `.to_owned()` is a direct allocation. The codebase already uses `.to_owned()` in many places, so this enforces consistency.

### Decision 4: `#[allow]` → `#[expect]` with reasons

**Choice:** Deny `allow_attributes` and `allow_attributes_without_reason`. Use `#[expect(..., reason = "...")]` for all suppressions.

**Rationale:** `#[expect]` warns when the suppressed lint no longer fires (unlike `#[allow]` which silently stays). The `reason` parameter documents intent for future readers.

### Decision 5: Allow `pub_use` in this proposal

**Choice:** `pub_use` is deferred to the `remove-module-name-repetitions` proposal.

**Rationale:** Denying `pub_use` requires renaming types to avoid import conflicts (two types named `Error`, two named `Plan`). This is a cascading change that touches every import in the codebase and should be done as a coordinated rename, not mixed with mechanical lint fixes.

**Update:** `pub_use` will be denied as part of proposal #2 together with `module_name_repetitions`.

### Decision 6: `print_stdout` with targeted suppression

**Choice:** Deny `print_stdout` globally, add `#[expect]` on the `Printer::print_lines` method and `main.rs` timer output.

**Rationale:** The `Printer` abstraction is the designated output channel. Raw `println!` in business logic would bypass it. The few legitimate uses get documented suppressions.

## Risks / Trade-offs

- **Risk: Large diff size (~800+ file changes)** → Mitigated by purely mechanical nature; each fix category is independently reviewable.
- **Risk: New clippy versions may change lint behavior** → Individual lint selection means new restriction lints won't auto-break. Only adopted lints are affected.
- **Risk: `#[expect]` annotations in tests create maintenance burden** → One annotation per test module, not per test function. ~20 annotations total.
- **Trade-off: `arithmetic_side_effects` may require sparse `#[expect]` annotations** → Accepted; the lint catches real overflow bugs and the false positives are few and documented.
