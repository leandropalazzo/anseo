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

    /// Manage scheduled Prompt Runs in opengeo.yaml.
    Schedule {
        #[command(subcommand)]
        sub: ScheduleSub,
    },

    /// Inspect the background worker.
    Worker {
        #[command(subcommand)]
        sub: WorkerSub,
    },

    /// Manage REST API access (Phase 2 Story 12.1).
    Api {
        #[command(subcommand)]
        sub: ApiSub,
    },

    /// Manage webhook delivery targets (Phase 2 Story 12.4).
    Webhook {
        #[command(subcommand)]
        sub: WebhookSub,
    },

    /// Manage public-benchmark contribution consent (Phase 2 Story 13.1).
    Benchmark {
        #[command(subcommand)]
        sub: BenchmarkSub,
    },

    /// Analytics backend management (Phase 2 Story 14.1).
    Analytics {
        #[command(subcommand)]
        sub: AnalyticsSub,
    },

    /// Plugin SDK substrate (Phase 3 Story 17.1). Manifest tooling only —
    /// install / load / sign land in later stories.
    Plugin {
        #[command(subcommand)]
        sub: PluginSub,
    },
}

#[derive(Debug, Subcommand)]
pub enum PluginSub {
    /// Validate a plugin manifest YAML on disk (substrate-only check: no
    /// signature verification, no host load).
    Validate(commands::plugin::ValidateArgs),
}

#[derive(Debug, Subcommand)]
pub enum AnalyticsSub {
    /// Migrate the current project's Postgres analytics view into
    /// ClickHouse pre-aggregated tables. Idempotent.
    MigrateToClickhouse(commands::analytics::MigrateArgs),
}

#[derive(Debug, Subcommand)]
pub enum BenchmarkSub {
    /// Opt this project into the public benchmark dataset. Prints the
    /// terms and prompts for confirmation; records consent in the
    /// `benchmark_consent` table.
    Optin(commands::benchmark::OptinArgs),
    /// Stop future contributions. Historical contributions remain per
    /// the contribution terms.
    Optout(commands::benchmark::OptoutArgs),
    /// Show current consent state (active / stale / not opted-in).
    Status(commands::benchmark::StatusArgs),
}

#[derive(Debug, Subcommand)]
pub enum WebhookSub {
    /// Declare a new webhook target. Generates and prints a fresh secret.
    Add(commands::webhook::AddArgs),
    /// List declared webhooks for the current project.
    List(commands::webhook::ListArgs),
    /// Generate and store a fresh secret; previous secret stops working.
    RotateSecret(commands::webhook::RotateSecretArgs),
    /// Re-enable a webhook that auto-disabled after permanent failures.
    Reenable(commands::webhook::ReenableArgs),
}

#[derive(Debug, Subcommand)]
pub enum ApiSub {
    /// API key management.
    Key {
        #[command(subcommand)]
        sub: ApiKeySub,
    },
}

#[derive(Debug, Subcommand)]
pub enum ApiKeySub {
    /// Generate a fresh API key. The plaintext is shown ONCE.
    Create(commands::api::CreateArgs),
    /// List API keys for the current project.
    List(commands::api::ListArgs),
    /// Revoke an API key by name.
    Revoke(commands::api::RevokeArgs),
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

#[derive(Debug, Subcommand)]
pub enum ScheduleSub {
    /// Add a Schedule declaration to opengeo.yaml.
    Add(commands::schedule::AddArgs),

    /// List Schedule declarations from opengeo.yaml.
    List(commands::schedule::ListArgs),

    /// Remove a Schedule declaration from opengeo.yaml.
    Remove(commands::schedule::RemoveArgs),
}

#[derive(Debug, Subcommand)]
pub enum WorkerSub {
    /// Print worker status.
    Status(commands::worker::StatusArgs),
}
