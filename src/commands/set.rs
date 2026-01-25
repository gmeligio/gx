use anyhow::Result;
use std::path::Path;

use crate::manifest::Manifest;
use crate::workflow::WorkflowUpdater;

pub fn execute(repo_root: &Path) -> Result<()> {
    let manifest = Manifest::load_from_repo(repo_root)?;

    if manifest.actions.is_empty() {
        println!("No actions defined in .github/gv.toml");
        return Ok(());
    }

    println!(
        "Loaded {} action(s) from .github/gv.toml",
        manifest.actions.len()
    );

    let updater = WorkflowUpdater::new(repo_root);
    let results = updater.update_all(&manifest.actions)?;

    if results.is_empty() {
        println!("No workflows were updated.");
    } else {
        for result in &results {
            println!("\nUpdated: {}", result.file.display());
            for change in &result.changes {
                println!("  - {}", change);
            }
        }
        println!("\n{} workflow(s) updated.", results.len());
    }

    Ok(())
}
