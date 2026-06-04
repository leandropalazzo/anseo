//! `ogeo audit <url>` — Epic 32 citation-readiness audit surface.

use std::time::Duration;

use clap::{Args, ValueEnum};
use opengeo_audit::{crawl_and_audit, evaluate_gate, AuditOptions, FailOn, GateSummary};
use opengeo_core::OpenGeoError;

#[derive(Debug, Args)]
pub struct AuditArgs {
    /// URL, sitemap URL, `file://` URL, or local HTML fixture path to audit.
    pub target: String,

    /// Output format: human report by default, JSON for CI and MCP consumers.
    #[arg(long, value_enum, default_value_t = AuditOutputFormat::Report)]
    pub format: AuditOutputFormat,

    /// Maximum same-origin pages to crawl.
    #[arg(long, default_value_t = 25)]
    pub max_pages: usize,

    /// Per-request timeout in milliseconds.
    #[arg(long, default_value_t = 10_000)]
    pub timeout_ms: u64,

    /// Fail the process when a violated rule matches this rule id or severity
    /// threshold (`low`, `medium`, `high`). May be repeated or comma-separated.
    #[arg(long = "fail-on", value_delimiter = ',')]
    pub fail_on: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AuditOutputFormat {
    Report,
    Json,
}

pub async fn run(args: AuditArgs) -> Result<(), OpenGeoError> {
    let options = AuditOptions {
        max_pages: args.max_pages,
        timeout: Duration::from_millis(args.timeout_ms),
    };
    let fail_on: Vec<FailOn> = args.fail_on.iter().map(|s| FailOn::parse(s)).collect();

    let report = crawl_and_audit(&args.target, options)
        .await
        .map_err(|e| OpenGeoError::Data(e.to_string()))?;
    let gate = (!fail_on.is_empty()).then(|| evaluate_gate(&report, &fail_on));
    let report = match gate.clone() {
        Some(gate) => report.with_gate(gate),
        None => report,
    };

    match args.format {
        AuditOutputFormat::Json => {
            let json = serde_json::to_string_pretty(&report)
                .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
            println!("{json}");
        }
        AuditOutputFormat::Report => print_report(&report),
    }

    if let Some(gate) = gate {
        if !gate.passed {
            if args.format == AuditOutputFormat::Report {
                print_gate_summary_json(&gate)?;
            }
            return Err(OpenGeoError::VisibilityCheckFailed(format!(
                "audit gate failed for {} finding(s)",
                gate.failed_findings.len()
            )));
        }
    }

    Ok(())
}

fn print_report(report: &opengeo_audit::AuditReport) {
    println!("OpenGEO audit report");
    println!("Target: {}", report.target);
    println!("Overall score: {}/100", report.overall_score);
    println!("Pages crawled: {}", report.pages.len());
    if let Some(gate) = &report.gate {
        println!(
            "Gate: {} ({})",
            if gate.passed { "pass" } else { "fail" },
            gate.fail_on.join(", ")
        );
    }
    println!();

    for page in &report.pages {
        println!("{} — {}/100", page.url, page.score);
        if let Some(title) = &page.title {
            println!("  title: {title}");
        }
        for finding in page.findings.iter().filter(|f| f.is_violation()) {
            println!(
                "  [{}/{}] {} — {}",
                finding.category.as_str(),
                finding.severity.as_str(),
                finding.rule_id,
                finding.message
            );
        }
        println!();
    }
}

fn print_gate_summary_json(gate: &GateSummary) -> Result<(), OpenGeoError> {
    let json =
        serde_json::to_string(gate).map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    eprintln!("{json}");
    Ok(())
}
