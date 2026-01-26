use anyhow::Result;
use clap::{Parser, Subcommand};
use gx::{commands, repo};

#[derive(Parser)]
#[command(name = "gx")]
#[command(about = "CLI to manage GitHub Actions dependencies", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Apply action versions from .github/gx.toml to all workflows
    Pin,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let repo_root = match repo::find_root() {
        Ok(root) => root,
        Err(e) if e.downcast_ref::<repo::GithubFolderNotFound>().is_some() => {
            println!(".github folder not found. gx didn't modify any file.");
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    match cli.command {
        Commands::Pin => commands::pin::run(&repo_root)?,
    }

    Ok(())
}
