use opengeo_core::telemetry::init_tracing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("opengeo-api")?;
    tracing::info!(
        event = "service.boot",
        service = "opengeo-api",
        "skeleton boot"
    );
    Ok(())
}
