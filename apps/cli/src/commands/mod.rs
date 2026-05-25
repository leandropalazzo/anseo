//! Subcommand implementations for the `ogeo` CLI.
//!
//! Each command module exposes one `run(args) -> Result<(), OpenGeoError>`
//! entry point used by both the binary main and integration tests.

pub mod check;
pub mod dashboard;
pub mod db;
pub mod init;
pub mod login;
pub mod prompt;
pub mod report;
pub mod run;
