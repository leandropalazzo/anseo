use opengeo_core::telemetry::init_tracing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("opengeo-worker")?;
    tracing::info!(
        event = "service.boot",
        service = "opengeo-worker",
        "skeleton boot"
    );
    Ok(())
}
