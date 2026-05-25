//! OpenGEO CLI library — the binary at `src/main.rs` is a thin wrapper that
//! parses args via clap and dispatches into this crate.
//!
//! Split into a library so integration tests in `apps/cli/tests/` can drive
//! commands without spawning the binary for cases where a Rust API is cleaner
//! than `assert_cmd` shell invocation.

pub mod commands;
pub mod scaffold;

use clap::{Parser, Subcommand};

/// Top-level `ogeo` CLI.
///
/// `clap` derives `--help` and `--version` for free. Every subcommand also
/// supports `--help` per the FR-10/FR-12 contract.
#[derive(Debug, Parser)]
#[command(
    name = "ogeo",
    version,
    about = "OpenGEO — track your brand's visibility in LLM responses.",
    long_about = "OpenGEO sends declared Prompts to declared Providers, extracts \
Mentions and Citations from the responses, and tracks how your Brand ranks against \
Competitors over time. Configure via opengeo.yaml; persist locally to PostgreSQL; \
view in the Dashboard."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Scaffold a new OpenGEO project in the current directory.
    Init(commands::init::InitArgs),

    /// Capture and persist a provider API key (FR-7, FR-8, FR-11).
    Login(commands::login::LoginArgs),

    /// Manage tracked Prompts in the project's opengeo.yaml.
    Prompt {
        #[command(subcommand)]
        sub: PromptSub,
    },

    /// Generate a summary report over a recent window (FR-14).
    Report {
        #[command(subcommand)]
        sub: ReportSub,
    },

    /// One-shot CI checks against persisted Prompt Run data (FR-15).
    Check {
        #[command(subcommand)]
        sub: CheckSub,
    },

    /// Open or print the URL of the local Dashboard (FR-16).
    Dashboard {
        #[command(subcommand)]
        sub: DashboardSub,
    },

    /// Database management (FR-21).
    Db {
        #[command(subcommand)]
        sub: commands::db::DbSub,
    },
}

#[derive(Debug, Subcommand)]
pub enum ReportSub {
    /// Produce a summary of recent Prompt Runs.
    Generate(commands::report::ReportArgs),
}

#[derive(Debug, Subcommand)]
pub enum CheckSub {
    /// Check that a Brand's Ranking stays at or below `--expect-rank-lte`.
    Visibility(commands::check::VisibilityArgs),
}

#[derive(Debug, Subcommand)]
pub enum DashboardSub {
    /// Open the dashboard in the default browser.
    Open(commands::dashboard::OpenArgs),
}

#[derive(Debug, Subcommand)]
pub enum PromptSub {
    /// Add a Prompt to opengeo.yaml.
    Add(commands::prompt::AddArgs),

    /// List Prompts declared in opengeo.yaml.
    List(commands::prompt::ListArgs),

    /// Execute declared Prompts × Providers (FR-2, FR-6, FR-13).
    Run(commands::run::RunArgs),
}
