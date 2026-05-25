//! `ogeo` — OpenGEO command-line interface.

use clap::Parser;
use opengeo_cli::{commands, CheckSub, Cli, Command, DashboardSub, PromptSub, ReportSub};
use opengeo_core::{telemetry::init_tracing, ExitCode, OpenGeoError};

fn main() {
    if let Err(e) = init_tracing("opengeo-cli") {
        eprintln!("failed to initialize tracing: {e}");
        std::process::exit(ExitCode::InternalError.into());
    }

    let cli = Cli::parse();

    let result: Result<(), OpenGeoError> = match cli.command {
        Command::Init(args) => commands::init::run(args),
        Command::Login(args) => commands::login::run(args),
        Command::Prompt { sub } => match sub {
            PromptSub::Add(args) => commands::prompt::run_add(args),
            PromptSub::List(args) => commands::prompt::run_list(args),
            PromptSub::Run(args) => run_async(commands::run::run(args)),
        },
        Command::Report { sub } => match sub {
            ReportSub::Generate(args) => commands::report::run(args),
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
