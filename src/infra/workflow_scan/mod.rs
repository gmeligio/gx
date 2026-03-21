#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Workflow file scanning and action extraction.
mod scanner;

pub use scanner::FileScanner;
