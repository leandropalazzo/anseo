//! `ogeo worker status` placeholder until Story 10.2 lands worker lifecycle.

use std::path::PathBuf;

use clap::Args;
use opengeo_core::OpenGeoError;

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Path to opengeo.yaml. Reserved for Story 10.2 worker lookup.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

pub fn run_status(_args: StatusArgs) -> Result<(), OpenGeoError> {
    println!("status: not-running");
    println!("queue_depth: unavailable");
    println!("last_tick: unavailable");
    println!("note: background worker execution lands in Story 10.2");
    Ok(())
}
