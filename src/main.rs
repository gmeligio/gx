use anyhow::Result;
use clap::{Parser, Subcommand};
use gx::commands;
use gx::infrastructure::{FileLock, LOCK_FILE_NAME, MemoryLock};
use gx::infrastructure::{FileManifest, MANIFEST_FILE_NAME, MemoryManifest};
use gx::infrastructure::{repo, repo::RepoError};
use log::{LevelFilter, info};
use std::io::Write;
use std::path::Path;

#[derive(Parser)]
#[command(name = "gx")]
#[command(about = "CLI to manage GitHub Actions dependencies", long_about = None)]
#[command(version)]
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
    /// Create manifest and lock files from current workflows
    Freeze,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    init_logging(&cli);

    let repo_root = match repo::find_root() {
        Ok(root) => root,
        Err(RepoError::GithubFolder) => {
            info!(".github folder not found. gx didn't modify any file.");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    let manifest_path = repo_root.join(".github").join(MANIFEST_FILE_NAME);
    let lock_path = repo_root.join(".github").join(LOCK_FILE_NAME);
    match cli.command {
        Commands::Tidy => {
            if manifest_path.exists() {
                run_file_backed(&repo_root, &manifest_path, &lock_path)
            } else {
                run_memory_only(&repo_root)
            }
        }
        Commands::Freeze => {
            if manifest_path.exists() {
                anyhow::bail!("Already frozen. Use `gx tidy` to update.");
            }
            info!("Freezing actions to manifest...");
            run_file_backed(&repo_root, &manifest_path, &lock_path)
        }
    }
}

/// Run tidy with file-backed manifest and lock (persists to disk)
fn run_file_backed(repo_root: &Path, manifest_path: &Path, lock_path: &Path) -> Result<()> {
    let manifest = FileManifest::load_or_default(manifest_path)?;
    let lock = FileLock::load_or_default(lock_path)?;
    commands::tidy::run(repo_root, manifest, lock)
}

/// Run tidy with in-memory manifest and lock (no persistence)
fn run_memory_only(repo_root: &Path) -> Result<()> {
    let manifest = MemoryManifest::default();
    let lock = MemoryLock::default();
    commands::tidy::run(repo_root, manifest, lock)
}

/// Initialize logging based on the verbosity level specified in the CLI
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
