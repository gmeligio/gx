//! The single source of truth for which files gx manages.
//!
//! Both the scanner (which reads `uses:` references) and the writer (which pins them)
//! discover files through here. Two implementations that could disagree would mean gx
//! knowing about a reference it never rewrites — the failure this module exists to
//! prevent.

use crate::domain::workflow::Error as WorkflowError;
use crate::domain::workflow_parsed::FileKind;
use glob::glob;
use std::path::{Path, PathBuf};

/// A managed file: its path on disk and the schema it follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedFile {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Which schema the file follows, decided by its location.
    pub kind: FileKind,
}

/// Find every file gx manages, in a stable order: workflows first, then action
/// definitions, each group sorted by path. Filesystem enumeration order is not stable
/// across machines, so sorting keeps output diffable between runs.
///
/// Workflows are `.github/workflows/*.{yml,yaml}` (flat, as GitHub requires). Action
/// definitions are `.github/actions/**/action.{yml,yaml}` (recursive, so a composite
/// action nested at any depth is found). Only `action.yml`/`action.yaml` is an action
/// definition — a sibling `config.yml` is not one and is left unread.
///
/// # Errors
///
/// Returns an error if a glob pattern cannot be compiled.
pub fn managed_files(repo_root: &Path) -> Result<Vec<ManagedFile>, WorkflowError> {
    let github = repo_root.join(".github");

    let mut workflows = glob_group(&github.join("workflows"), "*", FileKind::Workflow)?;
    workflows.sort_by(|a, b| a.path.cmp(&b.path));

    let mut actions = glob_group(
        &github.join("actions"),
        "**/action",
        FileKind::ActionDefinition,
    )?;
    actions.sort_by(|a, b| a.path.cmp(&b.path));

    workflows.extend(actions);
    Ok(workflows)
}

/// Glob one discovery root for both YAML extensions. `stem` is the file-name pattern
/// without extension, and may contain `**/` to recurse.
///
/// The `kind` is a property of the root, not of the file name — every hit under
/// `.github/actions` is an action definition and every hit under `.github/workflows` is
/// a workflow. This agrees with [`FileKind::of_path`], which is the single owner of that
/// question for callers arriving with a bare path; `discovery_kind_agrees_with_of_path`
/// pins the two together.
fn glob_group(dir: &Path, stem: &str, kind: FileKind) -> Result<Vec<ManagedFile>, WorkflowError> {
    let mut found = Vec::new();
    for extension in &["yml", "yaml"] {
        let pattern = dir
            .join(format!("{stem}.{extension}"))
            .to_string_lossy()
            .into_owned();
        let entries = glob(&pattern).map_err(|e| WorkflowError::ScanFailed {
            reason: e.to_string(),
        })?;
        // A per-entry read error (permissions, a vanished file) skips that entry rather
        // than aborting discovery, matching how per-file parse errors are handled.
        for path in entries.flatten() {
            found.push(ManagedFile { path, kind });
        }
    }
    Ok(found)
}

/// The paths of every managed file, in discovery order.
///
/// # Errors
///
/// Returns an error if a glob pattern cannot be compiled.
pub fn managed_paths(repo_root: &Path) -> Result<Vec<PathBuf>, WorkflowError> {
    Ok(managed_files(repo_root)?
        .into_iter()
        .map(|f| f.path)
        .collect())
}
