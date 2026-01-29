use anyhow::Result;
use clap::{Parser, Subcommand};
use env_logger::Env;
use gx::{commands, repo};

#[derive(Parser)]
#[command(name = "gx")]
#[command(about = "CLI to manage GitHub Actions dependencies", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ensure the manifest and lock matches the workflow code: add missing actions, remove unused, update workflows
    Tidy,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logger: use RUST_LOG env var, or default based on --verbose flag
    let default_level = if cli.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(Env::default().default_filter_or(default_level)).init();

    let repo_root = match repo::find_root() {
        Ok(root) => root,
        Err(e) if e.downcast_ref::<repo::GithubFolderNotFound>().is_some() => {
            log::info!(".github folder not found. gx didn't modify any file.");
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    match cli.command {
        Commands::Tidy => commands::tidy::run(&repo_root)?,
    }

    Ok(())
}
