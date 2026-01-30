use anyhow::Result;
use clap::{Parser, Subcommand};
use gx::{commands, error::GithubFolderNotFound, repo};
use log::LevelFilter;
use std::io::Write;

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

    let mut builder = env_logger::builder();
    builder
        .filter_level(match cli.verbose {
            true => LevelFilter::Debug,
            false => LevelFilter::Info,
        })
        .format(|buf, record| {
            let level = record.level();
            let style = &buf.default_level_style(level);

            writeln!(buf, "[{style}{level}{style:#}] {}", record.args())
        });

    if !cli.verbose {
        builder.format_timestamp(None);
    }

    builder.init();

    let repo_root = match repo::find_root() {
        Ok(root) => root,
        Err(e) if e.downcast_ref::<GithubFolderNotFound>().is_some() => {
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
