## Architecture

No new modules or layers. All changes modify existing code within the current architecture:

```
src/domain/resolution.rs    ← ActionResolver lifetime change
src/domain/lock.rs          ← is_complete destructuring, Option<RefType>
src/domain/action/uses_ref.rs ← RefType::parse() replaces From<&str>
src/output/lines.rs         ← format_line() method
src/output/printer.rs       ← simplified print_lines()
src/output/log_file.rs      ← time crate replaces hand-rolled datetime
src/infra/manifest.rs       ← try_from cast
src/infra/lock.rs           ← Option<RefType> in serialization/deserialization
src/tidy/mod.rs             ← &R signature
src/upgrade/mod.rs          ← &R signature
.github/workflows/build.yml ← integration job
tests/code_health.rs        ← guardrail test
```

## Design Decisions

### DD1: ActionResolver borrows instead of owning

`ActionResolver` changes from `ActionResolver<R>` with `registry: R` to `ActionResolver<'a, R>` with `registry: &'a R`.

The lifetime `'a` is always local to the `plan()` function. `ActionResolver` is never stored in a struct — it's created as a local in `plan()`, passed by `&resolver` to helper functions, and dropped at end of `plan()`. No lifetime escapes.

All `VersionRegistry` trait methods already take `&self`. `ShaIndex::get_or_describe` already takes `&R`. No trait changes needed.

Call sites in `Tidy::run()` and `Upgrade::run()` change from `registry` to `&registry`.

### DD2: format_line method on OutputLine

`OutputLine` gains a `pub fn format_line(&self, use_color: bool) -> String` method in `lines.rs`. This method contains the full match with symbol selection, color styling, and layout — all in one place per variant.

`Printer::print_lines()` becomes:

```rust
pub fn print_lines(&self, lines: &[OutputLine]) {
    for line in lines {
        println!("{}", line.format_line(self.use_color));
    }
}
```

The `console` crate import moves from `printer.rs` into `lines.rs` (or is used in both — `console` is already a dependency).

Rationale: keeping symbol + color + layout together per variant means adding a new variant requires editing one place. The printer has no variant-specific logic.

### DD3: time crate for datetime

Replace `secs_to_datetime()` (46 lines) and `is_leap()` (3 lines) with `time::OffsetDateTime::now_utc()`.

```rust
use time::OffsetDateTime;

fn chrono_now() -> String {
    let dt = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}",
        dt.year(), dt.month() as u8, dt.day(),
        dt.hour(), dt.minute(), dt.second()
    )
}

fn wall_clock_hms() -> String {
    let dt = OffsetDateTime::now_utc();
    format!("{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second())
}
```

Add `time = "0.3"` to `[dependencies]` in `Cargo.toml`.

### DD4: Option<RefType> with self-healing

`LockEntry.ref_type` changes from `RefType` to `Option<RefType>`.

`RefType::from(&str)` is replaced by `RefType::parse(&str) -> Option<RefType>`:

```rust
impl RefType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "release" => Some(RefType::Release),
            "tag" => Some(RefType::Tag),
            "branch" => Some(RefType::Branch),
            "commit" => Some(RefType::Commit),
            _ => None,
        }
    }
}
```

Remove the `From<String>` and `From<&str>` impls entirely.

**Deserialization** (`infra/lock.rs`): `lock_from_data` calls `RefType::parse()` instead of `RefType::from()`. Unknown values become `None`.

**Serialization** (`infra/lock.rs`): Write `ref_type` field as the variant name when `Some`, or `"unknown"` when `None`. This keeps the lock file parseable by older versions (which will default unknown to Tag — acceptable since the next tidy run with the new version will re-resolve).

**Migration**: `migrate_v1` sets `ref_type: None` instead of `"tag"` — the next `gx tidy` will resolve correctly via GitHub API.

**Construction**: `ResolvedAction` always has a known `RefType`, so `LockEntry` constructed from resolution gets `Some(ref_type)`.

### DD5: Exhaustive destructuring in is_complete

```rust
pub fn is_complete(&self, manifest_version: &Version) -> bool {
    let Self {
        sha: _,        // CommitSha is always valid by construction
        version,
        specifier,
        repository,
        ref_type,
        date,
    } = self;

    let version_ok = version.as_ref().is_some_and(|v| !v.is_empty());
    let repository_ok = !repository.is_empty();
    let date_ok = !date.is_empty();
    let ref_type_ok = ref_type.is_some();

    let expected_specifier = manifest_version.specifier();
    let specifier_ok = match specifier {
        Some(s) if s.is_empty() => false,
        actual => actual == &expected_specifier,
    };

    version_ok && repository_ok && date_ok && ref_type_ok && specifier_ok
}
```

Adding a field to `LockEntry` without listing it in the destructure is a compile error. The developer must explicitly decide to validate or skip (with `_` and a comment).

### DD6: Integration test CI job

New job in `build.yml`:

```yaml
integration:
    name: Integration
    runs-on: ubuntu-24.04
    steps:
        - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6
        - uses: actions-rust-lang/setup-rust-toolchain@1780873c7b576612439a134613cc4cc74ce5538c # v1.15.2
        - run: cargo test -- --ignored
          env:
            GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### DD7: Code health guardrail test

New file `tests/code_health.rs` with a recursive file reader that counts `#[ignore` occurrences in `src/` and fails if over 10. Uses only `std::fs` — zero dependencies.

TDD verification: first run with `max_ignored = 1` to confirm the test catches violations, then set to `max_ignored = 10`.

## Ripple Effects of Option<RefType>

Sites that access `entry.ref_type` and need updating:

| Location | Current usage | Change |
|----------|--------------|--------|
| `infra/lock.rs:119` | `RefType::from(entry_data.ref_type)` | `RefType::parse(&entry_data.ref_type)` |
| `infra/lock.rs:96` | `ref_type: "tag".to_string()` (v1 migration) | `ref_type: String::new()` or omit |
| `infra/lock.rs` serialization | `entry.ref_type` in format string | Handle `None` → write `"unknown"` |
| `infra/github.rs` lookup_sha | `if ref_type == RefType::Tag` | `if ref_type == Some(RefType::Tag)` |
| `domain/resolution.rs` resolve | `ref_type: RefType::Tag` | `ref_type: Some(RefType::Tag)` |
| `domain/resolution.rs` resolve_from_sha | `ref_type: RefType::Commit` / `RefType::Tag` | `ref_type: Some(...)` |
| `domain/lock.rs` constructors | `ref_type: RefType` param | `ref_type: Option<RefType>` param |
| `domain/lock.rs` is_complete | doesn't check ref_type | checks `ref_type.is_some()` |
| Test files | `RefType::Tag` in test data | `Some(RefType::Tag)` |
