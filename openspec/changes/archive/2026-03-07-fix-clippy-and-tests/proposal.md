## Why

The codebase has 7 `#[allow(clippy::...)]` directives suppressing pedantic lints and 2 `#[ignore]` tests that never run in CI. These are code quality debts: the `allow` directives mask real improvement opportunities, and the ignored tests provide zero coverage. Additionally, `is_complete()` doesn't check all `LockEntry` fields and there's no compile-time guarantee it will be updated when fields are added.

## What Changes

### 1. Remove `#[allow(clippy::needless_pass_by_value)]` (2 directives)

**Files:** `src/tidy/mod.rs:60`, `src/upgrade/mod.rs:112`

Refactor `ActionResolver<R>` to hold `&'a R` instead of owning `R`. The `plan()` functions take `&R` instead of `R`. This is correct because `ActionResolver` only ever calls `&self.registry` methods — it never consumes the registry.

**Changes:**
- `src/domain/resolution.rs`: `ActionResolver<R>` → `ActionResolver<'a, R>` with `registry: &'a R`
- `src/tidy/mod.rs`: `plan(registry: R)` → `plan(registry: &R)`, remove `#[allow]`
- `src/upgrade/mod.rs`: `plan(registry: R)` → `plan(registry: &R)`, remove `#[allow]`
- Call sites in `src/main.rs` or command `run()` functions: pass `&registry` instead of `registry`
- Test call sites: pass `&registry`

### 2. Remove `#[allow(clippy::too_many_lines)]` (1 directive)

**File:** `src/output/printer.rs:48`

Move the match body from `Printer::print_lines()` into `OutputLine::format_line(use_color: bool) -> String`. The printer becomes a 3-line loop. Each variant's formatting (symbol, color, layout) stays together in one match arm.

**Changes:**
- `src/output/lines.rs`: Add `pub fn format_line(&self, use_color: bool) -> String` to `OutputLine`
- `src/output/printer.rs`: Replace 100-line match with `println!("{}", line.format_line(self.use_color))`, remove `#[allow]`
- Move `console` dependency from printer to lines (or keep as re-export)
- Add unit tests for `format_line` in `lines.rs`

### 3. Remove `#[allow(clippy::cast_possible_truncation)]` (2 directives)

**File:** `src/output/log_file.rs:97,106`

Replace the hand-rolled `secs_to_datetime()` and `is_leap()` functions with the `time` crate.

**Changes:**
- `Cargo.toml`: Add `time = "0.3"` dependency
- `src/output/log_file.rs`: Delete `secs_to_datetime()` (~46 lines) and `is_leap()` (~3 lines). Replace with `time::OffsetDateTime::now_utc()` in `chrono_now()` and `wall_clock_hms()`

### 4. Remove `#[allow(clippy::cast_possible_wrap)]` (1 directive)

**File:** `src/infra/manifest.rs:534`

Replace `step as i64` with `i64::try_from(step).expect("step index overflow")`.

**Changes:**
- `src/infra/manifest.rs`: One-line change, remove `#[allow]`

### 5. Remove `#[allow(clippy::match_same_arms)]` (1 directive)

**File:** `src/domain/action/uses_ref.rs:39`

Make `ref_type` an `Option<RefType>` so unknown values become `None` instead of silently defaulting to `Tag`. This enables self-healing: incomplete entries trigger re-resolution on the next `gx tidy` run.

Additionally, use exhaustive destructuring in `is_complete()` so adding a field to `LockEntry` is a compile error until `is_complete()` is updated.

**Changes:**
- `src/domain/action/uses_ref.rs`: Replace `From<&str>` with `RefType::parse(s: &str) -> Option<RefType>` that returns `None` for unknown values. Remove `#[allow]`.
- `src/domain/lock.rs`: Change `ref_type: RefType` → `ref_type: Option<RefType>` in `LockEntry`. Rewrite `is_complete()` with exhaustive destructuring so the compiler enforces all-field coverage.
- `src/infra/lock.rs`: Update deserialization to use `RefType::parse()`, producing `None` for unrecognized values. Update serialization to write `ref_type` only when `Some`. Handle `None` in v1 migration (instead of hardcoding `"tag"`).
- All sites that read `entry.ref_type`: Update to handle `Option<RefType>`.
- Update the `From<&ResolvedAction>` or similar construction paths — resolved actions always have a known `RefType`, so they produce `Some(...)`.

### 6. Add integration test CI job and guardrail

**Files:** `.github/workflows/build.yml`, `tests/code_health.rs`

Add a CI job that runs `cargo test -- --ignored` with `GITHUB_TOKEN` so the 2 ignored tests actually run. Add a `tests/code_health.rs` test that counts `#[ignore]` attributes and fails if the count exceeds 10 — the standard Rust pattern for code health checks (used by rustc's `src/tools/tidy` and rust-analyzer).

**Changes:**
- `.github/workflows/build.yml`: Add `integration` job with `GITHUB_TOKEN` secret that runs `cargo test -- --ignored`
- `tests/code_health.rs`: Add `ignore_attribute_budget()` test. TDD: first verify it fails with `max_ignored = 1`, then set to `max_ignored = 10`.

## Capabilities

_(No user-facing behavior changes. Same commands, same output, same exit codes.)_

### Modified Capabilities

- **Lock self-healing**: Entries with unknown `ref_type` are now detected as incomplete and re-resolved on the next `gx tidy` run, instead of silently persisting a wrong default forever.

## Impact

- **`src/domain/resolution.rs`**: `ActionResolver` gains a lifetime parameter, holds `&'a R` instead of `R`
- **`src/domain/lock.rs`**: `LockEntry.ref_type` becomes `Option<RefType>`, `is_complete()` uses exhaustive destructuring
- **`src/domain/action/uses_ref.rs`**: `From<&str>` replaced with `RefType::parse() -> Option<RefType>`
- **`src/output/lines.rs`**: Gains `format_line()` method
- **`src/output/printer.rs`**: `print_lines()` shrinks from ~100 lines to ~3 lines
- **`src/output/log_file.rs`**: `secs_to_datetime()` + `is_leap()` deleted (~49 lines), replaced by `time` crate
- **`src/infra/manifest.rs`**: One-line cast fix
- **`src/tidy/mod.rs`**, **`src/upgrade/mod.rs`**: Signature change `registry: R` → `registry: &R`
- **`Cargo.toml`**: Add `time = "0.3"`
- **`.github/workflows/build.yml`**: Add `integration` job
- **`tests/code_health.rs`**: New file (~30 lines)

## Risks

- **`Option<RefType>` ripple**: Changing `ref_type` from `RefType` to `Option<RefType>` touches many sites. Each must be audited to handle `None` correctly.
- **Lock file compatibility**: Existing lock files with valid `ref_type` values are unaffected. Lock files from future versions with unknown ref_type values will now produce `None` instead of `Tag` — this is the desired behavior (triggers re-resolution) but is a semantic change.
- **`time` crate dependency**: Adds ~50KB to compile. Well-maintained (~230M downloads), no `libc` dependency.
- **`ActionResolver` lifetime**: The `'a` lifetime is contained within `plan()` — `ActionResolver` is only a local variable, never stored in a struct. Propagation risk is low.
