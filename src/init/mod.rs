#![expect(clippy::pub_use, reason = "reexport from extracted submodule")]

/// Init command: error types, struct, and `Command` implementation.
mod command;
pub mod report;

pub use command::{Error, Init};
