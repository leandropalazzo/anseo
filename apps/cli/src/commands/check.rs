//! `anseo check visibility` — FR-15.
//!
//! Phase 1 placeholder: argument parsing + structured "no data" result. Real
//! ranking-against-threshold logic lands once Story 3.2 ships mention
//! extraction + ranking computation.

use std::path::PathBuf;

use anseo_core::OpenGeoError;
use clap::Args;

#[derive(Debug, Args)]
pub struct VisibilityArgs {
    /// Prompt slug to check.
    #[arg(long)]
    pub prompt: String,

    /// Brand name (must match `brand.name` in opengeo.yaml).
    #[arg(long)]
    pub brand: String,

    /// Pass if Brand Ranking ≤ this value across every configured Provider's
    /// most-recent successful run.
    #[arg(long = "expect-rank-lte", value_name = "N")]
    pub expect_rank_lte: u32,

    /// Skip running a fresh Prompt Run; check only against persisted data.
    #[arg(long)]
    pub no_run: bool,

    /// Path to opengeo.yaml.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

pub fn run(args: VisibilityArgs) -> Result<(), OpenGeoError> {
    // Phase 1 stub: with no persistence yet (Story 3.1) we cannot evaluate.
    // Per FR-15 / PRD §11.4, "exit 2 if all configured Providers returned
    // errors". With zero providers reachable from this stub, we exit 2 so
    // CI invocations correctly treat the check as a hard failure until the
    // data layer comes online.
    let _ = (
        args.prompt,
        args.brand,
        args.expect_rank_lte,
        args.no_run,
        args.config,
    );
    Err(OpenGeoError::Provider {
        kind: anseo_core::ProviderErrorKind::NetworkError,
        message: "`anseo check visibility` requires persisted Prompt Runs; \
                  ships fully in Story 3.2 once extraction lands."
            .into(),
    })
}
