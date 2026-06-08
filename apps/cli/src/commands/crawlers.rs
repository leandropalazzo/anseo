//! `ogeo crawlers` — Roadmap Epic 31 crawler observability CLI.

use anseo_core::{Config, OpenGeoError};
use anseo_crawler_ingest::metrics::{CrawlerMetrics, MetricsParams, MetricsStore};
use anseo_storage::Storage;
use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub struct CrawlerArgs {
    #[arg(long, default_value = "anseo.yaml")]
    pub config: std::path::PathBuf,
    #[arg(long, default_value_t = 30)]
    pub days: i64,
    #[arg(long)]
    pub include_unverified: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,
    /// Show crawl-to-refer ratio instead of the standard crawler metrics.
    #[arg(long)]
    pub ratio: bool,
    /// Select the project by id (ULID) or brand name, overriding the working-dir
    /// `anseo.yaml` (ADR-004). Populated from the global `--project` flag.
    #[arg(skip)]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

pub async fn run(args: CrawlerArgs) -> Result<(), OpenGeoError> {
    let yaml = std::fs::read_to_string(&args.config).map_err(|e| {
        OpenGeoError::Config(format!("could not read {}: {e}", args.config.display()))
    })?;
    let config = Config::from_yaml_str(&yaml).map_err(|e| {
        OpenGeoError::Config(format!("could not parse {}: {e}", args.config.display()))
    })?;
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| OpenGeoError::Config("DATABASE_URL is required for `ogeo crawlers`".into()))?;
    let storage = Storage::connect(&database_url).await?;
    storage.migrate().await?;
    let store = MetricsStore::from_storage(&storage);
    let project_id =
        super::project::resolve_with_config(&storage, &config, args.project.as_deref()).await?;
    let params = MetricsParams {
        project_id,
        days: args.days,
        include_unverified: args.include_unverified,
    };

    if args.ratio {
        let report = store.fetch_crawl_refer_ratio(params).await?;
        match args.format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&report)
                    .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
                println!("{json}");
            }
            OutputFormat::Table => print_ratio_table(&report),
        }
        return Ok(());
    }

    let metrics = store.fetch(params).await?;

    match args.format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&metrics)
                .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
            println!("{json}");
        }
        OutputFormat::Table => print_table(&metrics),
    }
    Ok(())
}

fn print_ratio_table(report: &anseo_crawler_ingest::metrics::CrawlReferReport) {
    println!(
        "Crawl-to-refer ratio: {} to {} ({:?})",
        report.window_start.format("%Y-%m-%d"),
        report.window_end.format("%Y-%m-%d"),
        report.state
    );
    println!();
    println!(
        "{:<28} {:>16} {:>18} {:>12}",
        "BOT", "VERIFIED CRAWLS", "REFERRALS", "RATIO"
    );
    for bot in &report.bots {
        let ratio = bot
            .ratio
            .map(|v| format!("{v:.1}:1"))
            .unwrap_or_else(|| "crawls-only".into());
        println!(
            "{:<28} {:>16} {:>18} {:>12}",
            bot.bot_id, bot.verified_crawl_hits, bot.attributed_referrals, ratio
        );
    }
}

fn print_table(metrics: &CrawlerMetrics) {
    println!(
        "Crawler metrics: {} to {} ({})",
        metrics.window_start.format("%Y-%m-%d"),
        metrics.window_end.format("%Y-%m-%d"),
        if metrics.include_unverified {
            "including unverified"
        } else {
            "verified only"
        }
    );
    println!();
    println!(
        "{:<28} {:>10} {:>12} {:>10}",
        "BOT", "HITS", "VERIFIED", "ERRORS"
    );
    for bot in &metrics.bots {
        println!(
            "{:<28} {:>10} {:>12} {:>10}",
            bot.bot_id, bot.hits, bot.verified_hits, bot.error_hits
        );
    }

    println!();
    println!("Top crawled paths");
    println!("{:<52} {:>10} {:>10}", "PATH", "HITS", "ERRORS");
    for path in &metrics.top_paths {
        println!(
            "{:<52} {:>10} {:>10}",
            truncate(&path.path, 52),
            path.hits,
            path.error_hits
        );
    }

    println!();
    println!("Stuck/error pages");
    println!("{:<52} {:>10} {:>10}", "PATH", "HITS", "ERRORS");
    for path in &metrics.error_paths {
        println!(
            "{:<52} {:>10} {:>10}",
            truncate(&path.path, 52),
            path.hits,
            path.error_hits
        );
    }

    println!();
    println!("Trend");
    println!("{:<12} {:>10}", "DAY", "HITS");
    for bucket in &metrics.trend {
        println!("{:<12} {:>10}", bucket.day, bucket.hits);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('~');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_crawler_ingest::metrics::{BotMetric, PathMetric, TrendBucket};
    use chrono::Utc;

    #[test]
    fn json_shape_contains_metrics_sections() {
        let metrics = CrawlerMetrics {
            window_start: Utc::now(),
            window_end: Utc::now(),
            include_unverified: false,
            bots: vec![BotMetric {
                bot_id: "openai-gptbot".into(),
                hits: 2,
                verified_hits: 2,
                error_hits: 1,
            }],
            top_paths: vec![PathMetric {
                path: "/docs".into(),
                hits: 2,
                error_hits: 1,
            }],
            error_paths: vec![],
            trend: vec![TrendBucket {
                day: "2026-05-31".into(),
                hits: 2,
            }],
        };
        let json = serde_json::to_value(&metrics).unwrap();
        assert!(json.get("bots").is_some());
        assert!(json.get("top_paths").is_some());
        assert!(json.get("trend").is_some());
    }

    #[test]
    fn ratio_json_shape_names_degraded_state() {
        let report = anseo_crawler_ingest::metrics::CrawlReferReport {
            window_start: Utc::now(),
            window_end: Utc::now(),
            state: anseo_crawler_ingest::metrics::CrawlReferState::CrawlsOnly,
            bots: vec![],
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["state"], "crawls_only");
        assert!(json.get("bots").is_some());
    }
}
