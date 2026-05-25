//! `ogeo report generate` — FR-14.
//!
//! Phase 1 deliberately keeps this a thin shell over the storage layer; the
//! richer mention/citation/visibility roll-ups land with Epic 3.
//!
//! Without a DATABASE_URL we still respond with a structured empty report
//! (zero runs) so users running `ogeo init` followed by `ogeo report generate`
//! get a useful skeleton, not a panic.

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use opengeo_core::OpenGeoError;

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Output format. Default `human` (terse stdout summary).
    #[arg(long, value_enum, default_value_t = ReportFormat::Human)]
    pub format: ReportFormat,

    /// Window for the report (e.g. `24h`, `7d`, `30d`). Default `7d`.
    #[arg(long, default_value = "7d")]
    pub since: String,

    /// Path to opengeo.yaml. Defaults to `./opengeo.yaml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReportFormat {
    Human,
    Json,
    Markdown,
}

pub fn run(args: ReportArgs) -> Result<(), OpenGeoError> {
    // Parse --since for validation; we don't query DB rows yet but we want
    // bad inputs to fail at command-parse time.
    let _window = parse_duration(&args.since)?;

    let stub = serde_json::json!({
        "schema_version": "0.1",
        "window": args.since,
        "total_prompt_runs": 0,
        "succeeded": 0,
        "failed": 0,
        "by_provider": {},
        "by_prompt": {},
        "note": "Phase 1 placeholder. Persistence + mention/citation roll-ups land in Story 3.1+.",
    });

    match args.format {
        ReportFormat::Human => {
            println!("OpenGEO report (placeholder)");
            println!("  window:    {}", args.since);
            println!("  total runs: 0");
            println!("  succeeded:  0");
            println!("  failed:     0");
            println!();
            println!("Persistence + extraction land in Story 3.1+ — re-run once those are merged.");
        }
        ReportFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&stub).unwrap());
        }
        ReportFormat::Markdown => {
            println!("# OpenGEO report (placeholder)");
            println!();
            println!("| field | value |");
            println!("|---|---|");
            println!("| window | {} |", args.since);
            println!("| total runs | 0 |");
            println!("| succeeded | 0 |");
            println!("| failed | 0 |");
            println!();
            println!("_Persistence + extraction land in Story 3.1+._");
        }
    }
    Ok(())
}

/// Parse `7d` / `24h` / `60m` / `90s`. Returns the duration in seconds.
fn parse_duration(s: &str) -> Result<u64, OpenGeoError> {
    if s.is_empty() {
        return Err(OpenGeoError::Config("--since must not be empty".into()));
    }
    let (digits, suffix) = s.split_at(s.len() - 1);
    let n: u64 = digits.parse().map_err(|_| {
        OpenGeoError::Config(format!(
            "--since `{s}` must be of the form N[s|m|h|d] (e.g. 24h, 7d)"
        ))
    })?;
    let mult = match suffix {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => {
            return Err(OpenGeoError::Config(format!(
                "--since `{s}` has unknown unit; use s / m / h / d"
            )))
        }
    };
    Ok(n * mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_accepts_known_units() {
        assert_eq!(parse_duration("60s").unwrap(), 60);
        assert_eq!(parse_duration("5m").unwrap(), 300);
        assert_eq!(parse_duration("2h").unwrap(), 7200);
        assert_eq!(parse_duration("7d").unwrap(), 604_800);
    }

    #[test]
    fn parse_duration_rejects_garbage() {
        assert!(parse_duration("forever").is_err());
        assert!(parse_duration("5x").is_err());
        assert!(parse_duration("").is_err());
    }
}
