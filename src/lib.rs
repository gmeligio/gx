#![expect(
    unused_crate_dependencies,
    reason = "dev-dependencies are only used in integration tests"
)]

pub mod command;
pub mod config;
pub mod domain;
pub mod infra;
pub mod init;
pub mod lint;
pub mod output;
pub mod tidy;
pub mod upgrade;
