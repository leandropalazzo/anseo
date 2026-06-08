//! `ogeo report generate` — FR-14.
//!
//! Phase 1 deliberately keeps this a thin shell over the storage layer; the
//! richer mention/citation/visibility roll-ups land with Epic 3.
//!
//! Without a DATABASE_URL we still respond with a structured empty report
//! (zero runs) so users running `ogeo init` followed by `ogeo report generate`
//! get a useful skeleton, not a panic.

use std::path::PathBuf;

use anseo_analytics::sentiment::SentimentPoint;
use anseo_core::{Config, OpenGeoError};
use anseo_storage::Storage;
use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Output format. Default `human` (terse stdout summary).
    #[arg(long, value_enum, default_value_t = ReportFormat::Human)]
    pub format: ReportFormat,

    /// Window for the report (e.g. `24h`, `7d`, `30d`). Default `7d`.
    #[arg(long, default_value = "7d")]
    pub since: String,

    /// Path to anseo.yaml. Defaults to `./anseo.yaml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReportFormat {
    Human,
    Json,
    Markdown,
}

pub async fn run(args: ReportArgs) -> Result<(), OpenGeoError> {
    // Parse --since for validation; we don't query DB rows yet but we want
    // bad inputs to fail at command-parse time.
    let window = parse_duration(&args.since)?;
    let days = (window / 86_400).clamp(1, 365) as i32;
    let sentiment = load_sentiment(&args, days).await?;

    let stub = serde_json::json!({
        "schema_version": "0.1",
        "window": args.since,
        "total_prompt_runs": 0,
        "succeeded": 0,
        "failed": 0,
        "by_provider": {},
        "by_prompt": {},
        "sentiment": sentiment,
        "note": "Set DATABASE_URL and keep anseo.yaml available for live report aggregates.",
    });

    match args.format {
        ReportFormat::Human => {
            println!("OpenGEO report");
            println!("  window:    {}", args.since);
            println!("  total runs: 0");
            println!("  succeeded:  0");
            println!("  failed:     0");
            if sentiment.is_empty() {
                println!("  sentiment:  no classified mentions found");
            } else {
                println!("  sentiment:");
                for point in &sentiment {
                    println!(
                        "    {} / {} / {}: +{:.0}% ={:.0}% -{:.0}% avg {:.1}",
                        point.prompt,
                        point.provider,
                        point.entity,
                        point.positive_share * 100.0,
                        point.neutral_share * 100.0,
                        point.negative_share * 100.0,
                        point.average_score
                    );
                }
            }
        }
        ReportFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&stub).unwrap());
        }
        ReportFormat::Markdown => {
            println!("# OpenGEO report");
            println!();
            println!("| field | value |");
            println!("|---|---|");
            println!("| window | {} |", args.since);
            println!("| total runs | 0 |");
            println!("| succeeded | 0 |");
            println!("| failed | 0 |");
            println!();
            println!("## Sentiment");
            if sentiment.is_empty() {
                println!();
                println!("No classified mentions found.");
            } else {
                println!();
                println!(
                    "| prompt | provider | entity | positive | neutral | negative | avg score |"
                );
                println!("|---|---|---|---:|---:|---:|---:|");
                for point in &sentiment {
                    println!(
                        "| {} | {} | {} | {:.0}% | {:.0}% | {:.0}% | {:.1} |",
                        point.prompt,
                        point.provider,
                        point.entity,
                        point.positive_share * 100.0,
                        point.neutral_share * 100.0,
                        point.negative_share * 100.0,
                        point.average_score
                    );
                }
            }
        }
    }
    Ok(())
}

async fn load_sentiment(args: &ReportArgs, days: i32) -> Result<Vec<SentimentPoint>, OpenGeoError> {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        return Ok(Vec::new());
    };
    let path = args
        .config
        .as_deref()
        .unwrap_or_else(|| "anseo.yaml".as_ref());
    let yaml = match std::fs::read_to_string(path) {
        Ok(yaml) => yaml,
        Err(_) => return Ok(Vec::new()),
    };
    let cfg = Config::from_yaml_str(&yaml)
        .map_err(|e| OpenGeoError::Config(format!("could not parse {}: {e}", path.display())))?;
    let storage = Storage::connect(&url)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    anseo_analytics::sentiment::sentiment_points(&storage, cfg.project_id(), days)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))
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
