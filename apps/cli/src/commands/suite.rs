use anseo_benchmark::{canonical_geo_prompt_suite, canonical_prompt_by_slug};
use anseo_core::OpenGeoError;
use clap::Args;

#[derive(Debug, Args)]
pub struct ListArgs {}

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Canonical suite slug to validate, e.g. `geo-v1/best-vector-db`.
    #[arg(value_name = "SLUG")]
    pub slug: String,
}

pub fn run_list(_args: ListArgs) -> Result<(), OpenGeoError> {
    for entry in &canonical_geo_prompt_suite().entries {
        println!("{}", entry.slug);
    }
    Ok(())
}

pub fn run_check(args: CheckArgs) -> Result<(), OpenGeoError> {
    if canonical_prompt_by_slug(&args.slug).is_some() {
        println!("{}", args.slug);
        Ok(())
    } else {
        Err(OpenGeoError::SuiteCheckFailed(format!(
            "`{}` is not in the canonical GEO prompt suite",
            args.slug
        )))
    }
}
