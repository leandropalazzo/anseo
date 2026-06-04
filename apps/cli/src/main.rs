//! `ogeo` — OpenGEO command-line interface.

use clap::Parser;
use opengeo_cli::{
    commands, AnalyticsSub, ApiKeySub, ApiSub, BenchmarkSub, CheckSub, Cli, Command, DashboardSub,
    McpSub, PluginSub, ProjectSub, PromptSub, RecommendSub, ReportSub, ScheduleSub, WebhookSub,
    WorkerSub,
};
use opengeo_core::{telemetry::init_tracing, ExitCode, OpenGeoError};

fn main() {
    if let Err(e) = init_tracing("opengeo-cli") {
        eprintln!("failed to initialize tracing: {e}");
        std::process::exit(ExitCode::InternalError.into());
    }

    let cli = Cli::parse();
    // The global `--project` flag is threaded into the verbs that resolve a
    // project (ADR-004 precedence: it overrides the working-dir selector).
    let project_override = cli.project;

    let result: Result<(), OpenGeoError> = match cli.command {
        Command::Init(args) => commands::init::run(args),
        Command::Login(args) => commands::login::run(args),
        Command::Prompt { sub } => match sub {
            PromptSub::Add(args) => commands::prompt::run_add(args),
            PromptSub::List(args) => commands::prompt::run_list(args),
            PromptSub::Run(args) => run_async(commands::run::run(args)),
        },
        Command::Report { sub } => match sub {
            ReportSub::Generate(args) => run_async(commands::report::run(args)),
        },
        Command::Check { sub } => match sub {
            CheckSub::Visibility(args) => commands::check::run(args),
        },
        Command::Dashboard { sub } => match sub {
            DashboardSub::Open(args) => commands::dashboard::run(args),
        },
        Command::Db { sub } => match sub {
            commands::db::DbSub::Backup(args) => commands::db::run_backup(args),
            commands::db::DbSub::Restore(args) => commands::db::run_restore(args),
        },
        Command::Schedule { sub } => match sub {
            ScheduleSub::Add(args) => commands::schedule::run_add(args),
            ScheduleSub::List(args) => commands::schedule::run_list(args),
            ScheduleSub::Remove(args) => commands::schedule::run_remove(args),
        },
        Command::Worker { sub } => match sub {
            WorkerSub::Status(args) => commands::worker::run_status(args),
        },
        Command::Api { sub } => match sub {
            ApiSub::Key { sub } => match sub {
                ApiKeySub::Create(args) => run_async(commands::api::run_create(args)),
                ApiKeySub::List(args) => run_async(commands::api::run_list(args)),
                ApiKeySub::Revoke(args) => run_async(commands::api::run_revoke(args)),
            },
        },
        Command::Webhook { sub } => match sub {
            WebhookSub::Add(args) => run_async(commands::webhook::run_add(args)),
            WebhookSub::List(args) => run_async(commands::webhook::run_list(args)),
            WebhookSub::RotateSecret(args) => run_async(commands::webhook::run_rotate_secret(args)),
            WebhookSub::Reenable(args) => run_async(commands::webhook::run_reenable(args)),
        },
        Command::Benchmark { sub } => match sub {
            BenchmarkSub::Optin(args) => run_async(commands::benchmark::run_optin(args)),
            BenchmarkSub::Optout(args) => run_async(commands::benchmark::run_optout(args)),
            BenchmarkSub::Status(args) => run_async(commands::benchmark::run_status(args)),
        },
        Command::Analytics { sub } => match sub {
            AnalyticsSub::MigrateToClickhouse(args) => {
                run_async(commands::analytics::run_migrate(args))
            }
        },
        Command::Plugin { sub } => match sub {
            PluginSub::Validate(args) => commands::plugin::run_validate(args),
            PluginSub::Search(args) => commands::plugin::run_search(args),
            PluginSub::Install(args) => run_async(commands::plugin::run_install(args)),
            PluginSub::List(args) => run_async(commands::plugin::run_list(args)),
            PluginSub::Remove(args) => run_async(commands::plugin::run_remove(args)),
            PluginSub::Upgrade(args) => run_async(commands::plugin::run_upgrade(args)),
        },
        Command::Mcp { sub } => match sub {
            McpSub::Serve(args) => commands::mcp::run_serve(args),
            McpSub::Status(args) => commands::mcp::run_status(args),
            McpSub::InstallConfig(args) => commands::mcp::run_install_config(args),
        },
        Command::Recommend { sub } => match sub {
            RecommendSub::Generate(mut args) => {
                args.project = project_override.clone();
                run_async(commands::recommend::run_generate(args))
            }
            RecommendSub::List(args) => run_async(commands::recommend::run_list(args)),
            RecommendSub::Show(args) => run_async(commands::recommend::run_show(args)),
            RecommendSub::Ack(args) => run_async(commands::recommend::run_ack(args)),
            RecommendSub::Dismiss(args) => run_async(commands::recommend::run_dismiss(args)),
            RecommendSub::MarkActed(args) => run_async(commands::recommend::run_mark_acted(args)),
        },
        Command::Crawlers(mut args) => {
            args.project = project_override.clone();
            run_async(commands::crawlers::run(args))
        }
        Command::Audit(args) => run_async(commands::audit::run(args)),
        Command::Serve(args) => run_async(commands::serve::run(args)),
        Command::Project { sub } => match sub {
            ProjectSub::List(args) => run_async(commands::project::run_list(args)),
            ProjectSub::Create(args) => run_async(commands::project::run_create(args)),
            ProjectSub::Use(args) => run_async(commands::project::run_use(args)),
        },
    };

    match result {
        Ok(()) => std::process::exit(ExitCode::Success.into()),
        Err(err) => {
            let code = err.exit_code();
            eprintln!("error: {err}");
            std::process::exit(code.into());
        }
    }
}

fn run_async<F>(fut: F) -> Result<(), OpenGeoError>
where
    F: std::future::Future<Output = Result<(), OpenGeoError>>,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime initialization");
    runtime.block_on(fut)
}
