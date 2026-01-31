use anyhow::Result;
use clap::{Parser, Subcommand};
use gx::lock::{FileLock, LOCK_FILE_NAME, MemoryLock};
use gx::manifest::{FileManifest, MANIFEST_FILE_NAME, MemoryManifest};
use gx::{commands, repo, repo::RepoError};
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

    init_logging(&cli);

    let repo_root = match repo::find_root() {
        Ok(root) => root,
        Err(RepoError::GithubFolder()) => {
            log::info!(".github folder not found. gx didn't modify any file.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    match cli.command {
        Commands::Tidy => {
            let manifest_path = repo_root.join(".github").join(MANIFEST_FILE_NAME);
            let lock_path = repo_root.join(".github").join(LOCK_FILE_NAME);

            if manifest_path.exists() {
                // File-backed mode: use manifest and lock file
                let manifest = FileManifest::load_or_default(&manifest_path)?;
                let lock = FileLock::load_or_default(&lock_path)?;

                commands::tidy::run(&repo_root, manifest, lock)?;
            } else {
                // Memory-only mode: no manifest/lock persistence
                let manifest = MemoryManifest::default();
                let lock = MemoryLock::default();

                commands::tidy::run(&repo_root, manifest, lock)?;
            }
        }
    }

    Ok(())
}

fn init_logging(cli: &Cli) {
    let mut builder = env_logger::builder();
    builder
        .filter_level(if cli.verbose {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
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
}
