use opengeo_core::{telemetry::init_tracing, ExitCode};

fn main() {
    if let Err(e) = init_tracing("opengeo-cli") {
        eprintln!("failed to initialize tracing: {e}");
        std::process::exit(ExitCode::InternalError.into());
    }
    tracing::info!(
        event = "service.boot",
        service = "opengeo-cli",
        "skeleton boot"
    );
    std::process::exit(ExitCode::Success.into());
}
