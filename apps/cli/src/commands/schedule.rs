//! `ogeo schedule add|list|remove` for Phase 2 YAML v0.2 declarations.

use std::path::{Path, PathBuf};

use anseo_core::{Config, OpenGeoError, ProviderName, ScheduleConfig, SCHEMA_VERSION_V0_2};
use anseo_providers::cost::DEFAULT_PROJECT_MONTHLY_CAP_USD;
use anseo_scheduler::project_schedule_cost;
use chrono::Utc;
use clap::{Args, ValueEnum};

const DEFAULT_CONFIG_PATH: &str = "anseo.yaml";

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Schedule name (slug).
    #[arg(long)]
    pub name: String,

    /// Cron-style cadence or shorthand: hourly, daily, weekly, every N minutes, every N hours.
    #[arg(long)]
    pub cron: String,

    /// Prompt name to include. Repeatable.
    #[arg(long = "prompt")]
    pub prompts: Vec<String>,

    /// Provider name to include. Repeatable.
    #[arg(long = "provider")]
    pub providers: Vec<String>,

    /// Debounce window in minutes.
    #[arg(long, default_value_t = anseo_core::DEFAULT_SCHEDULE_DEBOUNCE_MINUTES)]
    pub debounce_minutes: u32,

    /// Allow projected monthly schedule cost above the project cap.
    #[arg(long)]
    pub allow_expensive: bool,

    /// Path to anseo.yaml. Defaults to `./anseo.yaml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = ListFormat::Table)]
    pub format: ListFormat,

    /// Path to anseo.yaml. Defaults to `./anseo.yaml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    /// Schedule name to remove.
    #[arg(long)]
    pub name: String,

    /// Path to anseo.yaml. Defaults to `./anseo.yaml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ListFormat {
    Table,
    Json,
}

pub fn run_add(args: AddArgs) -> Result<(), OpenGeoError> {
    let path = resolve_config_path(args.config);
    let mut cfg = Config::from_path(&path)?;

    if cfg.schedules.iter().any(|s| s.name == args.name) {
        return Err(OpenGeoError::Config(format!(
            "duplicate schedule name `{}`; pick a different slug",
            args.name
        )));
    }

    let providers = parse_providers(&args.providers)?;
    let mut schedule = ScheduleConfig {
        name: args.name,
        cron: args.cron,
        prompts: args.prompts,
        providers,
        debounce_minutes: args.debounce_minutes,
        projection_acknowledged_at: None,
    };
    let projection = project_schedule_cost(&schedule)
        .map_err(|e| OpenGeoError::Config(format!("invalid schedule: {e}")))?;

    if projection.cost.projected_monthly_usd > DEFAULT_PROJECT_MONTHLY_CAP_USD {
        if !args.allow_expensive {
            return Err(OpenGeoError::Config(format!(
                "projected monthly cost ${:.2} exceeds cap ${:.2}; rerun with --allow-expensive to acknowledge",
                projection.cost.projected_monthly_usd, DEFAULT_PROJECT_MONTHLY_CAP_USD
            )));
        }
        schedule.projection_acknowledged_at = Some(Utc::now().to_rfc3339());
    }

    cfg.schema_version = SCHEMA_VERSION_V0_2.to_string();
    cfg.schedules.push(schedule);
    let yaml = serde_yaml::to_string(&cfg)
        .map_err(|e| OpenGeoError::Config(format!("failed to serialize anseo.yaml: {e}")))?;
    let _round_trip = Config::from_yaml_str(&yaml)?;
    write_atomic(&path, &yaml)?;
    eprintln!(
        "Added schedule to {} (projected monthly cost ${:.2})",
        path.display(),
        projection.cost.projected_monthly_usd
    );
    Ok(())
}

pub fn run_list(args: ListArgs) -> Result<(), OpenGeoError> {
    let path = resolve_config_path(args.config);
    let cfg = Config::from_path(&path)?;
    match args.format {
        ListFormat::Json => {
            let rows: Vec<_> = cfg
                .schedules
                .iter()
                .map(|s| {
                    let projection = project_schedule_cost(s).ok();
                    serde_json::json!({
                        "name": s.name,
                        "cron": s.cron,
                        "prompts": s.prompts,
                        "providers": s.providers.iter().map(|p| p.as_wire_str()).collect::<Vec<_>>(),
                        "debounce_minutes": s.debounce_minutes,
                        "projection_acknowledged_at": s.projection_acknowledged_at,
                        "projected_monthly_usd": projection.map(|p| p.cost.projected_monthly_usd),
                    })
                })
                .collect();
            let json = serde_json::to_string_pretty(&rows).map_err(|e| {
                OpenGeoError::Internal(anyhow::anyhow!(
                    "failed to serialize schedules as JSON: {e}"
                ))
            })?;
            println!("{json}");
        }
        ListFormat::Table => print_table(&cfg),
    }
    Ok(())
}

pub fn run_remove(args: RemoveArgs) -> Result<(), OpenGeoError> {
    let path = resolve_config_path(args.config);
    let mut cfg = Config::from_path(&path)?;
    let before = cfg.schedules.len();
    cfg.schedules.retain(|s| s.name != args.name);
    if cfg.schedules.len() == before {
        return Err(OpenGeoError::Config(format!(
            "schedule `{}` is not declared",
            args.name
        )));
    }
    let yaml = serde_yaml::to_string(&cfg)
        .map_err(|e| OpenGeoError::Config(format!("failed to serialize anseo.yaml: {e}")))?;
    let _round_trip = Config::from_yaml_str(&yaml)?;
    write_atomic(&path, &yaml)?;
    eprintln!("Removed schedule `{}` from {}", args.name, path.display());
    Ok(())
}

fn print_table(cfg: &Config) {
    if cfg.schedules.is_empty() {
        println!("(no schedules declared)");
        return;
    }
    println!(
        "{:<28} {:<18} {:<9} {:<20} PROJECTED",
        "NAME", "CADENCE", "PROMPTS", "PROVIDERS"
    );
    println!(
        "{:<28} {:<18} {:<9} {:<20} ---------",
        "----", "-------", "-------", "---------"
    );
    for s in &cfg.schedules {
        let projected = project_schedule_cost(s)
            .map(|p| format!("${:.2}/mo", p.cost.projected_monthly_usd))
            .unwrap_or_else(|_| "invalid".into());
        let providers = s
            .providers
            .iter()
            .map(|p| p.as_wire_str())
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{:<28} {:<18} {:<9} {:<20} {}",
            s.name,
            s.cron,
            s.prompts.len(),
            truncate(&providers, 20),
            projected
        );
    }
}

fn parse_providers(values: &[String]) -> Result<Vec<ProviderName>, OpenGeoError> {
    if values.is_empty() {
        return Err(OpenGeoError::Config(
            "`ogeo schedule add` requires at least one --provider".into(),
        ));
    }
    let mut out = Vec::new();
    for value in values {
        let Some(provider) = ProviderName::parse(value) else {
            return Err(OpenGeoError::Config(format!(
                "unsupported provider `{value}`; expected one of {}",
                ProviderName::all_wire_names().join(", ")
            )));
        };
        if !out.contains(&provider) {
            out.push(provider);
        }
    }
    Ok(out)
}

fn resolve_config_path(arg: Option<PathBuf>) -> PathBuf {
    arg.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH))
}

fn write_atomic(path: &Path, contents: &str) -> Result<(), OpenGeoError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OpenGeoError::Config(format!(
                    "failed to create parent dir `{}`: {e}",
                    parent.display()
                ))
            })?;
        }
    }
    std::fs::write(path, contents)
        .map_err(|e| OpenGeoError::Config(format!("failed to write `{}`: {e}", path.display())))?;
    Ok(())
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}
