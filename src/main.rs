use anyhow::Result;
use clap::{Parser, Subcommand};
use gx::{commands, config::Config, repo};

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
    let config = Config::from_env().with_verbose(cli.verbose);

    let repo_root = match repo::find_root() {
        Ok(root) => root,
        Err(e) if e.downcast_ref::<repo::GithubFolderNotFound>().is_some() => {
            println!(".github folder not found. gx didn't modify any file.");
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    match cli.command {
        Commands::Tidy => commands::tidy::run(&repo_root, &config)?,
    }

    Ok(())
}
