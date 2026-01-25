use anyhow::Result;
use std::path::Path;

use crate::manifest::Manifest;
use crate::workflow::{UpdateResult, WorkflowUpdater};

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

    print_results(&results);

    Ok(())
}

fn print_results(results: &[UpdateResult]) {
    if results.is_empty() {
        println!("No workflows were updated.");
    } else {
        for result in results {
            println!("\nUpdated: {}", result.file.display());
            for change in &result.changes {
                println!("  - {}", change);
            }
        }
        println!("\n{} workflow(s) updated.", results.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_print_results_with_empty_results() {
        // This is a smoke test - just ensure it doesn't panic
        let results: Vec<UpdateResult> = vec![];
        print_results(&results);
    }

    #[test]
    fn test_print_results_with_updates() {
        // This is a smoke test - just ensure it doesn't panic
        let results = vec![UpdateResult {
            file: PathBuf::from("test.yml"),
            changes: vec!["actions/checkout@v4".to_string()],
        }];
        print_results(&results);
    }
}
