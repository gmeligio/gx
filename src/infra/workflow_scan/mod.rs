#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Which files gx manages — the single discovery source shared by scanner and writer.
pub mod discovery;
/// Managed file scanning and action extraction.
mod scanner;

pub use scanner::FileScanner;
