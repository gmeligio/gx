## Context

The prior change `clippy-restriction-lints` (2026-03-12) refactored the codebase to comply with 35 restriction lints but only committed 2 of them (`module_name_repetitions`, `pub_use`) to `Cargo.toml`. The remaining 33 lints were never gated. Production code still has 6 `expect()` calls and 1 dead `unwrap_or_else` + direct indexing.

## Goals / Non-Goals

**Goals:**
- Enable all 35 restriction lints in `Cargo.toml`
- Eliminate every `expect()` and `unwrap()` from production code through type system changes
- Introduce `StepIndex(u16)` as a domain type for workflow step positions
- Add `#[expect]` annotations to all test modules for safety lints

**Non-Goals:**
- Fixing violations that the prior change already resolved (code already complies)
- Changing any user-facing CLI behavior

## Decisions

### Decision 1: Pinned as a Scope variant (not a Mode)

**Choice:** Move `Pinned(Version)` from `Mode` to `Scope::Pinned(ActionId, Version)`.

**Rationale:** "Pinned" is not a mode of finding versions — it's a scope that targets a specific action at a specific version. The invalid combination `Pinned + All` exists only because mode and scope are independently combined when they shouldn't be. Making `Pinned` a scope variant eliminates the `Result` from `Request::new`, removes `Error::PinnedRequiresSingleScope`, and deletes the test for that error (the type system replaces it).

**Alternative:** Named constructors (`safe_all()`, `latest_single()`, etc.). Rejected because `new()` would still accept invalid combinations — the constructors just hide them. Option B makes the invalid state unrepresentable.

**After:**
```rust
enum Mode { Safe, Latest }
enum Scope {
    All,
    Single(ActionId),
    Pinned(ActionId, Version),
}
struct Request { pub mode: Mode, pub scope: Scope }
impl Request {
    fn new(mode: Mode, scope: Scope) -> Self { Self { mode, scope } }
}
```

### Decision 2: Delete Registry::default()

**Choice:** Remove `impl Default for Registry`.

**Rationale:** Zero callers in the codebase. All three call sites use `Registry::new(token)?`. The `Default` impl panics on TLS initialization failure — a `Default` that can panic is a design smell. Deleting it is the simplest and safest fix.

### Decision 3: StepIndex(u16) newtype

**Choice:** Introduce `StepIndex(u16)` as a domain type, replacing `Option<usize>` in `Location.step`, `ActionOverride.step`, and `ManifestEntryRaw.step`.

**Rationale:** Step indices in GitHub Actions workflows are always small (< 1000). Using `u16` makes `From<StepIndex> for i64` infallible, eliminating the `expect("step index overflow")` pattern. A newtype is preferred over bare `u16` for consistency with the existing domain types (`ActionId`, `CommitSha`, `Version`) and DDD philosophy.

**Alternative:** Bare `u16` without newtype. Rejected — less self-documenting and inconsistent with the domain modeling pattern.

### Decision 4: Fallible TOML array access

**Choice:** Replace `.as_array_mut().expect("override entry is always an array")` with `.as_array_mut().ok_or(ManifestError::Validation(...))?`.

**Rationale:** Even though the invariant holds (the entry was just created as an array), the function already returns `Result`. Propagating the error is zero-cost and makes the code robust against future refactoring that might change the creation logic.

### Decision 5: Simplify dominant_version

**Choice:** Replace `Version::highest(&candidates).unwrap_or_else(|| candidates[0].clone())` with `Version::highest(&candidates)`.

**Rationale:** The `unwrap_or_else` fallback and `candidates[0]` indexing are both dead code. `candidates` is guaranteed non-empty (it's filtered from a map where at least one entry equals `max_count`), and `Version::highest` returns `Some` for any non-empty slice. The fallback adds complexity and triggers `indexing_slicing` lint.

**Spec coverage:** This is a dead-code removal driven by the `code-quality` spec's requirements for safe indexing in non-test code and the `unwrap_used` lint. No new behavioral spec is needed — the function's return type (`Option<Version>`) and semantics are unchanged.

### Decision 6: Enable all 35 restriction lints

**Choice:** Add the remaining 33 lint entries to `Cargo.toml` as specified in the `clippy-restriction-config` spec.

**Rationale:** The codebase already complies from the prior refactor. This change just turns on the gate so regressions are caught at compile time.

## Risks / Trade-offs

- **Risk: Scope enum change touches pattern matching in plan.rs** — Contained to one file. The match arm for `Mode::Pinned` moves to `Scope::Pinned`.
- **Risk: StepIndex introduces a new type that all step-handling code must adopt** — Only 4 struct fields and ~6 conversion sites. The type is simple with `From`/`TryFrom` impls.
- **Risk: Enabling all 35 lints at once may surface violations missed by the prior change** — Mitigated by running clippy before committing. Any new violations would be from code added after the prior refactor.
