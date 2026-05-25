use opengeo_core::telemetry::init_tracing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("opengeo-mcp")?;
    tracing::info!(
        event = "service.boot",
        service = "opengeo-mcp",
        "skeleton boot"
    );
    Ok(())
}
