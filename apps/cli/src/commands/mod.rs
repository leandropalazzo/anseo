//! Subcommand implementations for the `ogeo` CLI.
//!
//! Each command module exposes one `run(args) -> Result<(), OpenGeoError>`
//! entry point used by both the binary main and integration tests.

pub mod analytics;
pub mod api;
pub mod benchmark;
pub mod check;
pub mod dashboard;
pub mod db;
pub mod init;
pub mod login;
pub mod plugin;
pub mod prompt;
pub mod report;
pub mod run;
pub mod schedule;
pub mod webhook;
pub mod worker;
