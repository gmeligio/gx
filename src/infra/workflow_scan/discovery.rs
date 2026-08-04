//! The single source of truth for which files gx manages, and for the schema each follows.
//!
//! Scanner and writer both discover files here. Two implementations could drift, leaving
//! gx aware of a reference it never rewrites.
//!
//! Kind is decided here and carried on [`ManagedFile`]; nothing downstream recomputes it
//! from a path. That is what lets an action definition outside `.github/actions` be read
//! as one — a path says where a file sits, not which schema it follows.

use crate::domain::file::parsed::FileKind;
use crate::domain::file::scan::Error as WorkflowError;
use glob::glob;
use std::path::{Path, PathBuf};

/// A managed file: its path on disk and the schema it follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedFile {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// Which schema the file follows, decided by the root that matched it and carried from
    /// here — never recomputed downstream.
    pub kind: FileKind,
}

/// Where one kind of managed file lives, relative to `.github`.
///
/// Pairing each glob with the kind it yields is what makes this the only place a kind is
/// decided; `discovery_assigns_kind_by_root` pins the assignment.
struct Root {
    /// Directory under `.github`.
    dir: &'static str,
    /// File-name pattern without extension. `**/` recurses.
    stem: &'static str,
    /// The schema every hit under `dir` follows.
    kind: FileKind,
}

/// Workflows are flat, as GitHub requires. Action definitions nest at any depth, and only
/// `action.{yml,yaml}` is one — a sibling `config.yml` is left unread.
const ROOTS: [Root; 2] = [
    Root {
        dir: "workflows",
        stem: "*",
        kind: FileKind::Workflow,
    },
    Root {
        dir: "actions",
        stem: "**/action",
        kind: FileKind::ActionDefinition,
    },
];

/// Find every file gx manages, in a stable order: workflows first, then action
/// definitions, each group sorted by path. Filesystem order varies by machine, so sorting
/// keeps output diffable between runs.
///
/// # Errors
///
/// Returns an error if a glob pattern cannot be compiled.
pub fn managed_files(repo_root: &Path) -> Result<Vec<ManagedFile>, WorkflowError> {
    let github = repo_root.join(".github");
    let mut found = Vec::new();

    for root in &ROOTS {
        let mut group = glob_root(&github.join(root.dir), root)?;
        group.sort_by(|a, b| a.path.cmp(&b.path));
        found.extend(group);
    }

    Ok(found)
}

/// Glob one discovery root for both YAML extensions.
fn glob_root(dir: &Path, root: &Root) -> Result<Vec<ManagedFile>, WorkflowError> {
    let mut found = Vec::new();
    for extension in &["yml", "yaml"] {
        let pattern = dir
            .join(format!("{}.{extension}", root.stem))
            .to_string_lossy()
            .into_owned();
        let entries = glob(&pattern).map_err(|e| WorkflowError::ScanFailed {
            reason: e.to_string(),
        })?;
        // A per-entry read error (permissions, a vanished file) skips that entry rather
        // than aborting discovery, matching how per-file parse errors are handled.
        for path in entries.flatten() {
            found.push(ManagedFile {
                path,
                kind: root.kind,
            });
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
