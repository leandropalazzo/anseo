//! OpenGEO CLI library — the binary at `src/main.rs` is a thin wrapper that
//! parses args via clap and dispatches into this crate.
//!
//! Split into a library so integration tests in `apps/cli/tests/` can drive
//! commands without spawning the binary for cases where a Rust API is cleaner
//! than `assert_cmd` shell invocation.

pub mod commands;
pub mod datastore;
pub mod handoff;
pub mod output;
pub mod preflight;
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
Competitors over time. Configure via anseo.yaml; persist locally to PostgreSQL; \
view in the Dashboard."
)]
pub struct Cli {
    /// Select the active project by id (ULID) or brand name. Overrides the
    /// working-dir `anseo.yaml` / `ogeo project use` selection on any verb
    /// that resolves a project (ADR-004 precedence; see `ogeo project`).
    #[arg(long, global = true)]
    pub project: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Scaffold a new OpenGEO project in the current directory.
    Init(commands::init::InitArgs),

    /// Capture and persist a provider API key (FR-7, FR-8, FR-11).
    Login(commands::login::LoginArgs),

    /// Manage tracked Prompts in the project's anseo.yaml.
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

    /// Manage scheduled Prompt Runs in anseo.yaml.
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

    /// Inspect the canonical GEO prompt suite used for benchmark-comparable slugs.
    Suite {
        #[command(subcommand)]
        sub: SuiteSub,
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

    /// MCP server management (Phase 3 Story 16.7).
    Mcp {
        #[command(subcommand)]
        sub: McpSub,
    },

    /// GEO recommendation verbs (Phase 3 Story 19.7).
    Recommend {
        #[command(subcommand)]
        sub: RecommendSub,
    },

    /// AI crawler observability: verified bot frequency, pages, and trends.
    Crawlers(commands::crawlers::CrawlerArgs),

    /// Crawl owned pages and score citation-readiness (Epic 32).
    Audit(commands::audit::AuditArgs),

    /// Run the HTTP API and the background worker in one process (Story 37.1).
    /// Requires `DATABASE_URL` (external Postgres). Binds `127.0.0.1` by default.
    Serve(commands::serve::ServeArgs),

    /// Manage projects: list, create, and select the working-dir default
    /// (Story 36.6). See ADR-004 for the selection precedence.
    Project {
        #[command(subcommand)]
        sub: ProjectSub,
    },

    /// Export a portable, checksummed archive of brands, prompts, and run
    /// history for self-host migration (Story 27.7). Provider keys are never
    /// included.
    Export {
        #[command(subcommand)]
        sub: commands::export::ExportSub,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectSub {
    /// List active (non-archived) projects.
    List(commands::project::ListArgs),
    /// Create a project from a brand name (derives its project_id).
    Create(commands::project::CreateArgs),
    /// Select a project as the working-dir default (persists a marker).
    Use(commands::project::UseArgs),
    /// Crypto-shred this project's benchmark key (IRREVERSIBLE). Renders every
    /// sealed contribution — native and ingested — permanently undecryptable
    /// (Story 40.4, GDPR Art.17 right-to-erasure).
    Shred(commands::project::ShredArgs),
}

#[derive(Debug, Subcommand)]
pub enum RecommendSub {
    /// Assemble live project facts, run the engine, and persist results.
    Generate(commands::recommend::GenerateArgs),
    /// List active recommendations for the current project.
    List(commands::recommend::ListArgs),
    /// Show one recommendation by id.
    Show(commands::recommend::ShowArgs),
    /// Acknowledge a surfaced recommendation.
    Ack(commands::recommend::AckArgs),
    /// Dismiss a recommendation.
    Dismiss(commands::recommend::DismissArgs),
    /// Mark a recommendation as acted, with optional evidence and note.
    MarkActed(commands::recommend::MarkActedArgs),
}

#[derive(Debug, Subcommand)]
pub enum PluginSub {
    /// Validate a plugin manifest YAML on disk (substrate-only check: no
    /// signature verification, no host load).
    Validate(commands::plugin::ValidateArgs),
    /// Search the registry index for plugins.
    Search(commands::plugin::SearchArgs),
    /// Download, signature-verify, and install a plugin.
    Install(commands::plugin::InstallArgs),
    /// List installed plugins.
    List(commands::plugin::ListArgs),
    /// Remove an installed plugin.
    Remove(commands::plugin::RemoveArgs),
    /// Upgrade an installed plugin to a new version.
    Upgrade(commands::plugin::UpgradeArgs),
    /// Generate an Ed25519 signing keypair (Story 41.4). Public key → pin as
    /// `ANSEO_ROOT_PUBKEY`; secret → store as the `ANSEO_PLUGIN_SIGNING_KEY`
    /// CI secret.
    Keygen(commands::plugin_sign::KeygenArgs),
    /// Sign a plugin bundle and emit its namespace claim (Story 41.4). Produces
    /// `signature.bin` + `claim.toml` that verify under the install path.
    Sign(commands::plugin_sign::SignArgs),
}

#[derive(Debug, Subcommand)]
pub enum McpSub {
    /// Start the MCP server (delegates to anseo-mcp binary).
    Serve(commands::mcp::ServeArgs),
    /// Show the status of a running MCP server.
    Status(commands::mcp::StatusArgs),
    /// List the registered MCP tools from the local `/v1/mcp/tools` catalog.
    Tools(commands::mcp::ToolsArgs),
    /// Write Claude Desktop / Cursor / Zed config snippet.
    InstallConfig(commands::mcp::InstallConfigArgs),
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
pub enum SuiteSub {
    /// Print the canonical GEO prompt slugs, one per line.
    List(commands::suite::ListArgs),
    /// Exit 0 when a slug is canonical; 1 when it is not.
    Check(commands::suite::CheckArgs),
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
    /// Add a Prompt to anseo.yaml.
    Add(commands::prompt::AddArgs),

    /// List Prompts declared in anseo.yaml.
    List(commands::prompt::ListArgs),

    /// Execute declared Prompts × Providers (FR-2, FR-6, FR-13).
    Run(commands::run::RunArgs),
}

#[derive(Debug, Subcommand)]
pub enum ScheduleSub {
    /// Add a Schedule declaration to anseo.yaml.
    Add(commands::schedule::AddArgs),

    /// List Schedule declarations from anseo.yaml.
    List(commands::schedule::ListArgs),

    /// Remove a Schedule declaration from anseo.yaml.
    Remove(commands::schedule::RemoveArgs),
}

#[derive(Debug, Subcommand)]
pub enum WorkerSub {
    /// Print worker status.
    Status(commands::worker::StatusArgs),
}
