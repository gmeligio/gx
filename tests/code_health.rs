#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::shadow_reuse,
    clippy::arithmetic_side_effects,
    reason = "tests use unwrap, indexing, and other patterns freely"
)]

use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Import path hygiene helpers
// ---------------------------------------------------------------------------

/// Find a sibling `.rs` file that includes `target` via `#[path = "target"]`.
/// Returns the includer's path and the module name declared for the target.
fn find_path_includer(target: &Path) -> Option<(std::path::PathBuf, String)> {
    let parent_dir = target.parent()?;
    let target_name = target.file_name()?.to_str()?;
    let path_attr = format!("#[path = \"{target_name}\"]");

    for entry in fs::read_dir(parent_dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some(target_name) {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let mut prev_was_path_attr = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed == path_attr {
                prev_was_path_attr = true;
                continue;
            }
            if prev_was_path_attr {
                // Next non-comment/non-attribute line should be `[pub(...)] mod <name>;`
                if trimmed.starts_with('#') {
                    // Skip attributes like #[cfg(test)]
                    continue;
                }
                // Strip optional visibility prefix to find `mod <name>;`
                let mod_part = trimmed
                    .strip_prefix("pub(crate) ")
                    .or_else(|| trimmed.strip_prefix("pub(super) "))
                    .or_else(|| trimmed.strip_prefix("pub "))
                    .unwrap_or(trimmed);
                if let Some(mod_name) = mod_part
                    .strip_prefix("mod ")
                    .and_then(|rest| rest.strip_suffix(';'))
                {
                    return Some((path, mod_name.trim().to_owned()));
                }
                prev_was_path_attr = false;
            }
        }
    }
    None
}

/// For a `tests.rs` file, find the sibling `.rs` file that contains `mod tests;`.
/// Returns the path to the includer, or `None` if not found.
fn find_tests_rs_includer(tests_file: &Path, _src_dir: &Path) -> Option<std::path::PathBuf> {
    let parent_dir = tests_file.parent()?;
    let entries = fs::read_dir(parent_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("tests.rs") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            // Skip comments
            if trimmed.starts_with("//") {
                continue;
            }
            // Match `mod tests;` (with optional #[cfg(test)] or #[path] on preceding lines)
            if trimmed == "mod tests;" {
                return Some(path);
            }
        }
    }
    None
}

/// Compute module path segments for a file relative to `src_dir`.
/// Handles `mod.rs`, `lib.rs`, `main.rs` stems (they represent the directory, not an extra segment).
fn module_path_segments(file_path: &Path, src_dir: &Path) -> Vec<String> {
    let rel = file_path.strip_prefix(src_dir);
    let parts: Vec<String> = rel
        .unwrap()
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(str::to_string))
        .collect();

    let mut segments: Vec<String> = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            let stem = part.strip_suffix(".rs").unwrap_or(part);
            if stem != "mod" && stem != "lib" && stem != "main" {
                segments.push(stem.to_owned());
            }
        } else {
            segments.push(part.clone());
        }
    }
    segments
}

/// Returns `(file_level_prefix, indented_prefix)` for depth-aware rule 2 checking.
///
/// - `file_level_prefix`: parent of the file's module (`crate::<grandparent>::...`)
/// - `indented_prefix`: the file's own module path (`crate::<parent>::...`)
///
/// For `tests.rs` files, resolves the actual includer to derive the correct module path.
fn parent_prefixes(file_path: &Path, src_dir: &Path) -> (Option<String>, Option<String>) {
    let is_tests_file = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "tests.rs");

    let segments = if is_tests_file {
        // For tests.rs, derive module path from the includer
        let base_segments = module_path_segments(file_path, src_dir);
        // base_segments ends with "tests" from the filename

        find_tests_rs_includer(file_path, src_dir).map_or(base_segments, |includer| {
            let includer_segments = module_path_segments(&includer, src_dir);
            // The tests.rs module path = includer's module path + "tests"
            let mut full = includer_segments;
            full.push("tests".to_owned());
            full
        })
    } else if let Some((includer, mod_name)) = find_path_includer(file_path) {
        // File is included via #[path = "..."] — derive module path from includer
        let mut includer_segments = module_path_segments(&includer, src_dir);
        includer_segments.push(mod_name);
        includer_segments
    } else {
        module_path_segments(file_path, src_dir)
    };

    if segments.len() <= 1 {
        // Parent is crate root — super:: not usable at any depth
        return (None, None);
    }

    // file_level_prefix = parent of the file's module
    let file_level_prefix = (segments.len() >= 2).then(|| {
        let parent = &segments[..segments.len() - 1];
        format!("crate::{}", parent.join("::"))
    });

    // indented_prefix = the file's own module path
    let indented_prefix = Some(format!("crate::{}", segments.join("::")));

    (file_level_prefix, indented_prefix)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Count non-test lines in a file, excluding `#[cfg(test)]` blocks and everything after.
///
/// Uses a simplified algorithm: finds the first `#[cfg(test)]` line and treats
/// everything from that point to EOF as test code. This assumes `#[cfg(test)]`
/// blocks always appear at the bottom of the file — a secondary assertion in
/// `logic_line_budget` validates this invariant.
///
/// Known limitation: a single-line `#[cfg(test)] use ...` would trigger the
/// cutoff too early. The invariant assertion catches this case.
fn count_non_test_lines(content: &str) -> usize {
    for (i, line) in content.lines().enumerate() {
        if line.trim().starts_with("#[cfg(test)]") {
            return i;
        }
    }
    content.lines().count()
}

/// Classifies mod.rs lines as structural (mod/use/comments/attributes/blanks)
/// vs. logic lines, with stateful tracking for multi-line `use {}` blocks.
struct ModRsScanner {
    in_use_block: bool,
    use_brace_depth: usize,
}

impl ModRsScanner {
    fn new() -> Self {
        Self {
            in_use_block: false,
            use_brace_depth: 0,
        }
    }

    fn is_structural_line(&mut self, line: &str) -> bool {
        const MOD_PREFIXES: &[&str] = &["mod ", "pub mod ", "pub(crate) mod ", "pub(super) mod "];
        const USE_PREFIXES: &[&str] = &["use ", "pub use ", "pub(crate) use ", "pub(super) use "];

        let trimmed = line.trim();

        // Inside a multi-line use block — track brace depth
        if self.in_use_block {
            let opens = trimmed.chars().filter(|&c| c == '{').count();
            let closes = trimmed.chars().filter(|&c| c == '}').count();
            if closes > self.use_brace_depth + opens {
                self.use_brace_depth = 0;
            } else {
                self.use_brace_depth = self.use_brace_depth + opens - closes;
            }
            if self.use_brace_depth == 0 {
                self.in_use_block = false;
            }
            return true;
        }

        // Blank lines
        if trimmed.is_empty() {
            return true;
        }

        // Comments
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with("*/") {
            return true;
        }

        // Attributes
        if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            return true;
        }

        // Module declarations
        for prefix in MOD_PREFIXES {
            if trimmed.starts_with(prefix) {
                return true;
            }
        }

        // Use/reexport statements
        for prefix in USE_PREFIXES {
            if trimmed.starts_with(prefix) {
                // Check if this opens a multi-line use block
                let opens = trimmed.chars().filter(|&c| c == '{').count();
                let closes = trimmed.chars().filter(|&c| c == '}').count();
                if opens > closes {
                    self.in_use_block = true;
                    self.use_brace_depth = opens - closes;
                }
                return true;
            }
        }

        false
    }
}

/// Recursively collect all `.rs` files under `dir` (excluding `code_health.rs` itself).
fn collect_rs_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_rs_files(&path));
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && path.file_name().and_then(|n| n.to_str()) != Some("code_health.rs")
            {
                files.push(path);
            }
        }
    }
    files
}

fn count_ignore_attributes(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_ignore_attributes(&path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && path.file_name().and_then(|n| n.to_str()) != Some("code_health.rs")
                && let Ok(content) = fs::read_to_string(&path)
            {
                count += content.matches("#[ignore").count();
            }
        }
    }
    count
}

#[test]
fn ignore_attribute_budget() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_count = count_ignore_attributes(&manifest_dir.join("src"));
    let tests_count = count_ignore_attributes(&manifest_dir.join("tests"));
    let count = src_count + tests_count;
    let max_ignored = 0;
    assert!(
        count <= max_ignored,
        "Too many #[ignore] attributes in src/ and tests/: found {count} (src: {src_count}, tests: {tests_count}), budget is {max_ignored}. \
         Remove #[ignore] from tests that can now run, or increase the budget if justified."
    );
}

// ---------------------------------------------------------------------------
// Task 1.1 — Layer dependency direction
// ---------------------------------------------------------------------------

/// Domain modules must not import from command modules or infra.
///
/// The allowed dependency direction is: command → domain ← infra.
/// Domain must remain pure and free of upward or sideways dependencies.
#[test]
fn domain_does_not_import_upward() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let domain_dir = manifest_dir.join("src").join("domain");

    let forbidden = &[
        "crate::tidy",
        "crate::upgrade",
        "crate::lint",
        "crate::init",
        "crate::infra",
    ];

    let mut violations: Vec<String> = Vec::new();

    for file in collect_rs_files(&domain_dir) {
        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            for &forbidden_import in forbidden {
                if trimmed.contains(forbidden_import) {
                    violations.push(format!(
                        "{}: forbidden import `{forbidden_import}`",
                        file.display()
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Domain layer imports from forbidden modules:\n  {}",
        violations.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Task 1.2 — Duplicate private function detection across command modules
// ---------------------------------------------------------------------------

/// Private (non-`pub`) functions with the same name across different command
/// modules signal logic that belongs in the domain layer.
///
/// Same-named functions within the same module (e.g. tidy/mod.rs and tidy/diff.rs)
/// are allowed. Public functions are intentionally excluded.
#[test]
fn no_duplicate_private_fns_across_command_modules() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let command_modules = &["tidy", "upgrade", "lint", "init"];

    // Map: fn_name → list of (module_name, file_path)
    let mut fn_to_modules: HashMap<String, Vec<String>> = HashMap::new();

    for module in command_modules {
        let module_dir = src_dir.join(module);
        for file in collect_rs_files(&module_dir) {
            let Ok(content) = fs::read_to_string(&file) else {
                continue;
            };
            // Track whether we're inside an `impl Trait for Type` block via brace depth.
            let mut in_trait_impl = false;
            let mut trait_impl_depth: usize = 0;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("//") {
                    continue;
                }
                // Detect start of a trait impl block
                if !in_trait_impl && trimmed.starts_with("impl ") && trimmed.contains(" for ") {
                    in_trait_impl = true;
                    trait_impl_depth = 0;
                }
                if in_trait_impl {
                    let opens = trimmed.chars().filter(|&c| c == '{').count();
                    let closes = trimmed.chars().filter(|&c| c == '}').count();
                    if opens >= closes {
                        trait_impl_depth += opens - closes;
                    } else {
                        trait_impl_depth -= closes - opens;
                    }
                    if trait_impl_depth == 0 {
                        in_trait_impl = false;
                        trait_impl_depth = 0;
                    }
                    // Skip fn lines inside trait impl blocks
                    continue;
                }
                // Match `fn name(` but not `pub fn`, `pub(crate) fn`, async fn with pub, etc.
                if trimmed.starts_with("fn ")
                    && !trimmed.starts_with("pub")
                    && let Some(name) = trimmed
                        .strip_prefix("fn ")
                        .and_then(|rest| rest.split('(').next())
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                {
                    fn_to_modules
                        .entry(name.to_owned())
                        .or_default()
                        .push(module.to_string());
                }
            }
        }
    }

    let mut violations: Vec<String> = Vec::new();
    for (fn_name, modules) in &fn_to_modules {
        // Deduplicate modules to check cross-module duplication only
        let mut unique_modules: Vec<&str> = modules.iter().map(String::as_str).collect();
        unique_modules.sort_unstable();
        unique_modules.dedup();
        if unique_modules.len() > 1 {
            violations.push(format!(
                "`{fn_name}` appears in: {}",
                unique_modules.join(", ")
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Duplicate private functions across command modules (consider moving to domain):\n  {}",
        violations.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Task 1.3 — File size budget
// ---------------------------------------------------------------------------

/// No `.rs` file in `src/` should exceed the line budget.
///
/// Current budget is set to the current maximum + margin while large files
/// are being split. Target is 500 lines per file.
#[test]
fn file_size_budget() {
    // TODO: lower further once infra/github.rs and upgrade/mod.rs are split.
    //       Domain splits (manifest.rs, lock.rs, identity.rs, resolution.rs) are done.
    //       identity.rs grew with Repository, VersionComment, CommitDate newtypes.
    let max_lines: usize = 550;

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut violations: Vec<String> = Vec::new();

    for file in collect_rs_files(&src_dir) {
        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
        let line_count = content.lines().count();
        if line_count > max_lines {
            violations.push(format!("{}: {line_count} lines", file.display()));
        }
    }

    assert!(
        violations.is_empty(),
        "Files exceeding {max_lines}-line budget (target: 500):\n  {}",
        violations.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Task 1.4 — Folder file count budget
// ---------------------------------------------------------------------------

// Count only direct .rs files per directory (non-recursive)
fn count_rs_in_dir(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    let p = e.path();
                    p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("rs")
                })
                .count()
        })
        .unwrap_or(0)
}

fn check_dir(dir: &Path, max_files: usize, violations: &mut Vec<String>) {
    let count = count_rs_in_dir(dir);
    if count > max_files {
        violations.push(format!("{}: {count} .rs files", dir.display()));
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                check_dir(&path, max_files, violations);
            }
        }
    }
}

/// Import paths must follow three rules:
///
/// 1. No `super::super::` — use `crate::` instead
/// 2. No `use crate::<parent>::` when `use super::` suffices (target is one hop up)
/// 3. No `use self::` — it is always redundant
#[test]
fn import_path_hygiene() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut violations: Vec<String> = Vec::new();

    for file in collect_rs_files(&src_dir) {
        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
        let (file_level_prefix, indented_prefix) = parent_prefixes(&file, &src_dir);

        for (lineno, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }

            // Rule 1: ban super::super:: anywhere (use crate:: instead)
            if trimmed.contains("super::super::") {
                violations.push(format!(
                    "{}:{}: rule 1 (use crate:: instead of super::super::): {trimmed}",
                    file.display(),
                    lineno + 1,
                ));
            }

            if !trimmed.contains("use ") {
                continue;
            }

            // Rule 2: ban `use crate::<parent>::` when `use super::` suffices.
            // Uses indent-based prefix selection:
            // - indent 0 → file_level_prefix (parent of file's module)
            // - indent 4+ → indented_prefix (file's own module, for inline mod blocks)
            let indent = line.len() - line.trim_start().len();
            let prefix = if indent == 0 {
                &file_level_prefix
            } else {
                &indented_prefix
            };
            if let Some(prefix) = prefix
                && let Some(after_use) = trimmed.split("use ").nth(1)
            {
                let check = format!("{prefix}::");
                if after_use.starts_with(&check) {
                    violations.push(format!(
                        "{}:{}: rule 2 (use super:: instead of {prefix}::): {trimmed}",
                        file.display(),
                        lineno + 1,
                    ));
                }
            }

            // Rule 3: ban use self:: (redundant)
            if trimmed.contains("use self::") {
                violations.push(format!(
                    "{}:{}: rule 3 (remove self:: prefix): {trimmed}",
                    file.display(),
                    lineno + 1,
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Import path hygiene violations:\n  {}",
        violations.join("\n  ")
    );
}

/// No directory in `src/` should contain more than the file count budget.
///
/// Current budget is set to current maximum + margin while the domain module
/// is being reorganized. Target is 8 files per directory.
#[test]
fn folder_file_count_budget() {
    let max_files: usize = 8;

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut violations: Vec<String> = Vec::new();
    check_dir(&src_dir, max_files, &mut violations);

    assert!(
        violations.is_empty(),
        "Directories exceeding {max_files}-file budget (target: 8):\n  {}",
        violations.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Task 1.5 — Logic line budget
// ---------------------------------------------------------------------------

/// No `.rs` file in `src/` should exceed the logic line budget.
///
/// Logic lines = total lines minus `#[cfg(test)]` blocks (everything from the
/// first `#[cfg(test)]` to EOF). Standalone `tests.rs` files are excluded
/// entirely (they are 100% test code).
///
/// Budget: 440 (current max: 438 in infra/github/resolve.rs).
/// Target: 300 once large files are split.
#[test]
fn logic_line_budget() {
    let max_logic_lines: usize = 440;
    // Target: 300 once infra/github/resolve.rs and lint/mod.rs are split.

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut violations: Vec<String> = Vec::new();
    let mut invariant_violations: Vec<String> = Vec::new();

    for file in collect_rs_files(&src_dir) {
        // Skip standalone tests.rs files — entirely test code
        if file.file_name().and_then(|n| n.to_str()) == Some("tests.rs") {
            continue;
        }

        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };

        let logic_lines = count_non_test_lines(&content);

        // Secondary assertion: validate the "all production code precedes #[cfg(test)]"
        // invariant. Check that no top-level item declaration (at column 0) appears
        // after the first #[cfg(test)] line. Test module contents are indented, so
        // only truly top-level production code at column 0 would indicate a violation.
        let total_lines: Vec<&str> = content.lines().collect();
        if logic_lines < total_lines.len() {
            const TOP_LEVEL_ITEM_PREFIXES: &[&str] = &[
                "pub fn ",
                "pub(crate) fn ",
                "pub async fn ",
                "pub(crate) async fn ",
                "pub struct ",
                "pub enum ",
                "pub type ",
                "pub const ",
                "pub static ",
                "pub trait ",
                "pub impl ", // not valid Rust but catches copy-paste errors
            ];
            for (offset, line) in total_lines[logic_lines..].iter().enumerate() {
                // Only check lines at column 0 (no leading whitespace) — test
                // module contents are indented and won't match.
                if line.starts_with(char::is_whitespace) || line.is_empty() {
                    continue;
                }
                let trimmed = line.trim();
                for prefix in TOP_LEVEL_ITEM_PREFIXES {
                    if trimmed.starts_with(prefix) {
                        invariant_violations.push(format!(
                            "{}:{}: production code after #[cfg(test)] — \
                             upgrade to general brace-tracking algorithm. \
                             Line: {trimmed}",
                            file.strip_prefix(manifest_dir).unwrap_or(&file).display(),
                            logic_lines + offset + 1,
                        ));
                        break;
                    }
                }
            }
        }

        if logic_lines > max_logic_lines {
            violations.push(format!(
                "{}: {logic_lines} logic lines",
                file.strip_prefix(manifest_dir).unwrap_or(&file).display()
            ));
        }
    }

    assert!(
        invariant_violations.is_empty(),
        "cfg(test)-at-bottom invariant violated (simplified counting is wrong):\n  {}",
        invariant_violations.join("\n  ")
    );

    assert!(
        violations.is_empty(),
        "Files exceeding {max_logic_lines}-logic-line budget (target: 300):\n  {}",
        violations.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Task 1.6 — mod.rs reexports only
// ---------------------------------------------------------------------------

/// Every `mod.rs` should ideally contain only reexports, module declarations,
/// attributes, and comments. Logic belongs in named files.
///
/// Budget: 360 per file (current max: 354 in lint/mod.rs).
/// Target: 0 (mod.rs should be reexports only).
#[test]
fn mod_rs_reexports_only() {
    let max_mod_logic: usize = 360;
    // Target: 0 (mod.rs should be reexports only).
    // Current max: 354 (lint/mod.rs). Headroom for minor multi-line use edge cases.

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut report: Vec<(String, usize)> = Vec::new();

    for file in collect_rs_files(&src_dir) {
        if file.file_name().and_then(|n| n.to_str()) != Some("mod.rs") {
            continue;
        }

        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };

        let non_test_lines = count_non_test_lines(&content);
        let lines: Vec<&str> = content.lines().collect();
        let mut scanner = ModRsScanner::new();
        let mut logic_count: usize = 0;

        for line in lines.iter().take(non_test_lines) {
            if !scanner.is_structural_line(line) {
                logic_count += 1;
            }
        }

        if logic_count > 0 {
            let display = file
                .strip_prefix(manifest_dir)
                .unwrap_or(&file)
                .display()
                .to_string();
            report.push((display, logic_count));
        }
    }

    // Sort by logic count descending for visibility
    report.sort_by(|a, b| b.1.cmp(&a.1));

    let over_budget: Vec<String> = report
        .iter()
        .filter(|(_, count)| *count > max_mod_logic)
        .map(|(path, count)| format!("{path}: {count} logic lines"))
        .collect();

    let report_str: String = report
        .iter()
        .map(|(path, count)| format!("  {path}: {count} logic lines"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        over_budget.is_empty(),
        "mod.rs files with logic (budget: {max_mod_logic}, target: 0):\n{report_str}\n\n\
         Over budget:\n  {}",
        over_budget.join("\n  ")
    );

    if !report.is_empty() {
        eprintln!("mod.rs files with logic (budget: {max_mod_logic}, target: 0):\n{report_str}");
    }
}

// ---------------------------------------------------------------------------
// Task 1.7 — No generic file names
// ---------------------------------------------------------------------------

/// File names should describe what the code does, not what kind of code it is.
///
/// Budget: 1 (current: upgrade/types.rs).
/// Target: 0.
#[test]
fn no_generic_file_names() {
    let max_generic_names: usize = 1;
    // Target: 0.
    // Current violation: upgrade/types.rs

    let denied = [
        "types.rs",
        "utils.rs",
        "helpers.rs",
        "common.rs",
        "misc.rs",
        "consts.rs",
        "constants.rs",
    ];

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut violations: Vec<String> = Vec::new();

    for file in collect_rs_files(&src_dir) {
        if let Some(name) = file.file_name().and_then(|n| n.to_str())
            && denied.contains(&name)
        {
            violations.push(
                file.strip_prefix(manifest_dir)
                    .unwrap_or(&file)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        violations.len() <= max_generic_names,
        "Generic file names (budget: {max_generic_names}, target: 0):\n  {}",
        violations.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Inline tests for helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod helper_tests {
    use super::*;

    // -- count_non_test_lines tests --

    #[test]
    fn no_test_block() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        assert_eq!(count_non_test_lines(content), 3);
    }

    #[test]
    fn inline_test_block_at_eof() {
        let content = "fn foo() {}\nfn bar() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn it_works() {}\n}\n";
        assert_eq!(count_non_test_lines(content), 2);
    }

    #[test]
    fn cfg_test_mod_tests_declaration() {
        // #[cfg(test)] followed by mod tests; (single line, no brace block)
        let content = "fn foo() {}\n#[cfg(test)]\nmod tests;\n";
        assert_eq!(count_non_test_lines(content), 1);
    }

    #[test]
    fn empty_file() {
        assert_eq!(count_non_test_lines(""), 0);
    }

    #[test]
    fn cfg_test_in_string_literal_documents_limitation() {
        // Known limitation: #[cfg(test)] in a string literal triggers cutoff
        let content = "let s = \"\n#[cfg(test)]\n\";\nfn real_code() {}\n";
        // The simplified algorithm cuts at line 1 (the #[cfg(test)] line)
        assert_eq!(count_non_test_lines(content), 1);
    }

    #[test]
    fn cfg_test_in_doc_comment_documents_limitation() {
        // Known limitation: line starting with #[cfg(test)] in a doc-comment context
        // But doc comments start with /// so this wouldn't match
        let content = "/// Example:\n/// #[cfg(test)]\nfn foo() {}\n";
        // /// #[cfg(test)] doesn't start with #[cfg(test)] after trim, so no cutoff
        assert_eq!(count_non_test_lines(content), 3);
    }

    #[test]
    fn cfg_test_in_macro_body_documents_limitation() {
        // Known limitation: if a macro body has #[cfg(test)] at line start
        let content = "macro_rules! m {\n    () => {\n#[cfg(test)]\n    };\n}\n";
        // The simplified algorithm cuts at line 2 (the #[cfg(test)] line)
        assert_eq!(count_non_test_lines(content), 2);
    }

    // -- ModRsScanner prefix classification tests --

    #[test]
    fn structural_mod_declarations() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line("mod foo;"));
        assert!(s.is_structural_line("pub mod bar;"));
        assert!(s.is_structural_line("pub(crate) mod baz;"));
        assert!(s.is_structural_line("pub(super) mod qux;"));
    }

    #[test]
    fn structural_use_statements() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line("use crate::foo;"));
        assert!(s.is_structural_line("pub use crate::bar;"));
        assert!(s.is_structural_line("pub(crate) use super::baz;"));
        assert!(s.is_structural_line("pub(super) use super::qux;"));
    }

    #[test]
    fn structural_comments() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line("// a comment"));
        assert!(s.is_structural_line("/// doc comment"));
        assert!(s.is_structural_line("//! module doc"));
        assert!(s.is_structural_line("/* block comment start"));
        assert!(s.is_structural_line("*/"));
    }

    #[test]
    fn structural_attributes() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line("#[derive(Debug)]"));
        assert!(s.is_structural_line("#![allow(unused)]"));
    }

    #[test]
    fn structural_blank_lines() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line(""));
        assert!(s.is_structural_line("   "));
    }

    #[test]
    fn logic_lines_detected() {
        let mut s = ModRsScanner::new();
        assert!(!s.is_structural_line("fn foo() {}"));
        assert!(!s.is_structural_line("struct Bar;"));
        assert!(!s.is_structural_line("let x = 1;"));
        assert!(!s.is_structural_line("impl Foo {"));
    }

    // -- ModRsScanner multi-line use block tests --

    #[test]
    fn single_line_use_no_state_change() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line("use crate::{Foo, Bar};"));
        assert!(!s.in_use_block);
    }

    #[test]
    fn multi_line_use_block() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line("use crate::{"));
        assert!(s.in_use_block);
        assert!(s.is_structural_line("    Foo,"));
        assert!(s.in_use_block);
        assert!(s.is_structural_line("    Bar,"));
        assert!(s.in_use_block);
        assert!(s.is_structural_line("};"));
        assert!(!s.in_use_block);
    }

    #[test]
    fn nested_braces_in_use_block() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line("use crate::{"));
        assert!(s.is_structural_line("    foo::{Bar, Baz},"));
        assert!(s.in_use_block);
        assert!(s.is_structural_line("    Qux,"));
        assert!(s.is_structural_line("};"));
        assert!(!s.in_use_block);
    }

    #[test]
    fn interleaved_logic_after_use_block() {
        let mut s = ModRsScanner::new();
        assert!(s.is_structural_line("use crate::Foo;"));
        assert!(!s.is_structural_line("fn bar() {}"));
        assert!(s.is_structural_line("use crate::Baz;"));
    }
}
