## Context

The project has strict clippy configuration (`pedantic`, `perf`, `nursery` all `deny`, plus `rust-2018-idioms`). After the edition 2024 upgrade, 113 errors surfaced. All are mechanical fixes — no architectural changes needed.

## Goals / Non-Goals

**Goals:**
- Fix all 113 clippy errors so `mise run clippy` passes cleanly
- Keep the strict lint configuration unchanged in Cargo.toml

**Non-Goals:**
- Refactoring or restructuring code
- Changing any runtime behavior
- Adding or removing lint rules

## Decisions

### Group fixes by lint rule, not by file

**Rationale:** Each lint rule requires the same mechanical transformation everywhere. Grouping by rule lets each fix batch be applied consistently and reviewed as a cohesive change. It also enables parallel agents to work on non-overlapping lint groups simultaneously without merge conflicts within a single file.

**Alternative considered:** Grouping by file — rejected because the same lint (e.g., `use_self`) spans 20+ files and reviewing one file with mixed fix types is harder than reviewing one lint type across all files.

### Fix order: largest groups first, independent groups in parallel

1. **`use_self`** (62 errors) — replace `TypeName::Variant` with `Self::Variant` inside `impl` blocks
2. **`missing_const_for_fn`** (21 errors) — add `const` keyword to trivial constructors/accessors
3. **`option_if_let_else`** (10 errors) — rewrite `if let Some` to `map_or_else` / `map_or`
4. **`elided_lifetimes_in_paths`** (5 errors) — add `<'_>` to generic types missing lifetime annotations
5. **`redundant_clone`** (3 errors) — remove unnecessary `.clone()` calls
6. **`non_binding_let`** (3 errors) — use `let _guard =` or `drop()` for types with destructors
7. **Misc one-offs** (9 errors) — `derive_partial_eq_without_eq`, `or_fun_call`, `unused_mut`, `needless_collect`, `redundant_pub_crate`, `doc_markdown`

Groups 1-3 can be parallelized (they touch different code patterns within each file). Groups 4-7 are small enough to do sequentially.

### Apply clippy's own suggestions verbatim

**Rationale:** Every error has a clippy-provided fix suggestion. Using these verbatim ensures correctness and avoids introducing new issues. No creative interpretation needed.

## Risks / Trade-offs

- **`option_if_let_else` readability** → Some `map_or_else` rewrites with closures may be less readable than the original `if let`. Accept this as the cost of keeping nursery lints at deny level. If a specific case is truly unreadable, it can be locally `#[allow()]`'d with a comment.
- **`missing_const_for_fn` stability** → `const fn` capabilities expand with each Rust release. Some functions may need `const` removed if their body changes later to use non-const operations. Low risk given these are simple field assignments.
