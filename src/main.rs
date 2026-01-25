use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env;
use std::path::PathBuf;

mod commands;
mod manifest;
mod workflow;

#[derive(Parser)]
#[command(name = "gv")]
#[command(about = "CLI to manage GitHub Actions dependencies", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Apply action versions from .github/gv.toml to all workflows
    Set,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = find_repo_root()?;

    match cli.command {
        Commands::Set => commands::set::execute(&repo_root)?,
    }

    Ok(())
}

fn find_repo_root() -> Result<PathBuf> {
    let current_dir = env::current_dir()?;

    let mut dir = current_dir.as_path();
    loop {
        if dir.join(".github").exists() {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    Ok(current_dir)
}
