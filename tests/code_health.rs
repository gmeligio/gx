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
    let max_lines: usize = 500;

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
