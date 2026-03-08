## Tasks

Tasks are ordered so each group can be built and tested independently. Groups 1-4 are independent of each other and can be done in any order. Group 5 (Option<RefType>) depends on understanding the full codebase. Group 6 (CI + guardrail) is independent.

---

### Group 1: ActionResolver borrows registry (removes 2 `#[allow]`)

- [x] 1.1 Change `ActionResolver<R>` to `ActionResolver<'a, R>` with `registry: &'a R` in `src/domain/resolution.rs`. Update `new()` to take `&'a R`. Update `registry()` return type.
- [x] 1.2 Change `tidy::plan()` signature from `registry: R` to `registry: &R` in `src/tidy/mod.rs`. Remove `#[allow(clippy::needless_pass_by_value)]`. Update `ActionResolver::new(registry)` call (no `&` needed since it's already a ref).
- [x] 1.3 Change `upgrade::plan()` signature from `registry: R` to `registry: &R` in `src/upgrade/mod.rs`. Remove `#[allow(clippy::needless_pass_by_value)]`. Update `ActionResolver::new(registry)` call.
- [x] 1.4 Update call sites in `Tidy::run()` (`src/tidy/mod.rs`) and `Upgrade::run()` (`src/upgrade/mod.rs`): pass `&registry` instead of `registry`.
- [x] 1.5 Update all integration test call sites in `tests/tidy_test.rs` and `tests/upgrade_test.rs`: pass `&registry` to `plan()`.
- [x] 1.6 Update unit test call sites in `src/tidy/mod.rs` and `src/upgrade/mod.rs` test modules: pass `&registry` to `plan()`.
- [x] 1.7 Run `cargo test` and `cargo clippy` — verify no `needless_pass_by_value` warnings and all tests pass.

---

### Group 2: OutputLine::format_line (removes 1 `#[allow]`)

- [x] 2.1 Add `pub fn format_line(&self, use_color: bool) -> String` to `OutputLine` in `src/output/lines.rs`. Move the full match body from `Printer::print_lines()` — each arm returns a `format!(...)` string instead of calling `println!()` directly. Import `console::style` in `lines.rs`.
- [x] 2.2 Replace `Printer::print_lines()` body in `src/output/printer.rs` with a loop calling `line.format_line(self.use_color)` and `println!`. Remove `#[allow(clippy::too_many_lines)]`. Remove the `console::style` import from `printer.rs` if no longer needed.
- [x] 2.3 Add unit tests for `format_line` in `src/output/lines.rs`: test at least `Upgraded`, `LintDiag`, `Summary`, and `Blank` variants with `use_color = false`.
- [x] 2.4 Run `cargo test` and `cargo clippy` — verify no `too_many_lines` warning and all tests pass.

---

### Group 3: time crate (removes 2 `#[allow]`)

- [x] 3.1 Add `time = "0.3"` to `[dependencies]` in `Cargo.toml`.
- [x] 3.2 Replace `chrono_now()` and `wall_clock_hms()` in `src/output/log_file.rs` with `time::OffsetDateTime::now_utc()`. Delete `secs_to_datetime()` and `is_leap()`. Remove both `#[allow(clippy::cast_possible_truncation)]`.
- [x] 3.3 Run `cargo test` and `cargo clippy` — verify no `cast_possible_truncation` warnings and all tests pass.

---

### Group 4: try_from cast (removes 1 `#[allow]`)

- [x] 4.1 In `src/infra/manifest.rs:534`, replace `#[allow(clippy::cast_possible_wrap)]` and `(step as i64)` with `i64::try_from(step).expect("step index overflow")`.
- [x] 4.2 Run `cargo test` and `cargo clippy` — verify no `cast_possible_wrap` warning.

---

### Group 5: Option<RefType> + exhaustive is_complete (removes 1 `#[allow]`)

- [x] 5.1 In `src/domain/action/uses_ref.rs`: remove `From<&str>` and `From<String>` impls. Add `pub fn parse(s: &str) -> Option<Self>` method on `RefType`. Remove `#[allow(clippy::match_same_arms)]`.
- [x] 5.2 Update `RefType` tests in `uses_ref.rs`: change `RefType::from("unknown")` assertion from `== RefType::Tag` to test `RefType::parse("unknown") == None`. Test all four known values return `Some(...)`.
- [x] 5.3 In `src/domain/lock.rs`: change `ref_type: RefType` to `ref_type: Option<RefType>` in `LockEntry`. Update `new()` and `with_version_and_specifier()` parameter types to `Option<RefType>`.
- [x] 5.4 Rewrite `is_complete()` in `src/domain/lock.rs` with exhaustive `let Self { sha: _, version, specifier, repository, ref_type, date } = self;` destructuring. Add `ref_type.is_some()` check.
- [x] 5.5 Update `is_complete` tests: add test for `ref_type: None` returning incomplete. Update existing tests to use `Some(RefType::Tag)` etc.
- [x] 5.6 In `src/infra/lock.rs`: update `lock_from_data()` to call `RefType::parse(&entry_data.ref_type)` instead of `RefType::from(entry_data.ref_type)`.
- [x] 5.7 In `src/infra/lock.rs`: update `migrate_v1()` to set `ref_type: String::new()` (so it parses as `None`, triggering re-resolution on next tidy).
- [x] 5.8 In `src/infra/lock.rs`: update serialization (`serialize_lock` and `apply_lock_diff`) to handle `Option<RefType>` — write the variant name when `Some`, write `"unknown"` when `None`.
- [x] 5.9 In `src/infra/github.rs`: update `lookup_sha()` where `ref_type` is compared (`if ref_type == RefType::Tag` → `if ref_type == Some(RefType::Tag)`). Update `resolve_ref()` return to wrap in `Some(...)`.
- [x] 5.10 In `src/domain/resolution.rs`: update `ResolvedRef.ref_type` to `Option<RefType>`. Update `resolve()` and `resolve_from_sha()` to return `Some(RefType::...)`.
- [x] 5.11 Update `ResolvedAction` in `src/domain/plan.rs` (or wherever it lives): change `ref_type: RefType` to `ref_type: Option<RefType>`.
- [x] 5.12 Update `Lock::set()` in `src/domain/lock.rs` to pass `Some(resolved.ref_type.clone())` or adjust if `ResolvedAction` already has `Option<RefType>`.
- [x] 5.13 Update all test files (`tests/tidy_test.rs`, `tests/upgrade_test.rs`, `tests/e2e_test.rs`, etc.): change `RefType::Tag` to `Some(RefType::Tag)` in mock data and assertions.
- [x] 5.14 Run `cargo test` and `cargo clippy` — verify no `match_same_arms` warning, all tests pass, and entries with `None` ref_type are detected as incomplete.

---

### Group 6: Integration tests CI + guardrail

- [x] 6.1 Create `tests/code_health.rs` with `ignore_attribute_budget()` test, initially with `max_ignored = 1`. Run `cargo test --test code_health` — verify it **fails** (we have 2 ignored tests, budget is 1). This proves the guardrail works.
- [x] 6.2 Change `max_ignored` to `10` in `tests/code_health.rs`. Run `cargo test --test code_health` — verify it **passes**.
- [x] 6.3 Add `integration` job to `.github/workflows/build.yml` that runs `cargo test -- --ignored` with `GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}`.
- [x] 6.4 Run full `cargo test` — verify all tests pass including the new code_health test.

---

### Final Verification

- [x] 7.1 Run `cargo clippy` — verify zero `#[allow(clippy::...)]` directives remain (except the two crate-level `#![allow(unused_crate_dependencies)]` in `lib.rs` and `main.rs`).
- [x] 7.2 Run `cargo test` — verify all tests pass.
