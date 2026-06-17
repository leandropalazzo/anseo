//! Subcommand implementations for the `ogeo` CLI.
//!
//! Each command module exposes one `run(args) -> Result<(), OpenGeoError>`
//! entry point used by both the binary main and integration tests.

pub mod analytics;
pub mod api;
pub mod audit;
pub mod benchmark;
pub mod check;
pub mod crawlers;
pub mod dashboard;
pub mod db;
pub mod init;
pub mod login;
pub mod mcp;
pub mod plugin;
pub mod plugin_install;
pub mod plugin_registry;
pub mod plugin_sign;
pub mod project;
pub mod prompt;
pub mod recommend;
pub mod report;
pub mod run;
pub mod schedule;
pub mod serve;
pub mod suite;
pub mod webhook;
pub mod worker;
