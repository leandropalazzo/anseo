//! Schedule declaration validation + background-worker substrate for Phase 2.

pub mod events;
pub mod notifications;
pub mod transport;
pub mod webhooks;
pub mod worker;

use opengeo_core::{Config, ProviderName, ScheduleConfig};
use opengeo_providers::cost::{project_monthly_cost, CostProjection};
use thiserror::Error;

pub const DEFAULT_MAX_TICKS_PER_HOUR_PER_SCHEDULE: f64 = 12.0;
pub const DEFAULT_MAX_TICKS_PER_DAY_PER_PROVIDER: f64 = 96.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cadence {
    pub ticks_per_day: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DensityCaps {
    pub max_ticks_per_hour_per_schedule: f64,
    pub max_ticks_per_day_per_provider: f64,
}

impl Default for DensityCaps {
    fn default() -> Self {
        Self {
            max_ticks_per_hour_per_schedule: DEFAULT_MAX_TICKS_PER_HOUR_PER_SCHEDULE,
            max_ticks_per_day_per_provider: DEFAULT_MAX_TICKS_PER_DAY_PER_PROVIDER,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleProjection {
    pub cadence: Cadence,
    pub cost: CostProjection,
}

#[derive(Debug, Error, PartialEq)]
pub enum ScheduleValidationError {
    #[error("unsupported schedule cadence `{0}`; expected hourly, daily, weekly, every N minutes, every N hours, or a simple 5-field cron")]
    UnsupportedCadence(String),
    #[error("schedule `{schedule}` exceeds per-schedule density cap: {ticks_per_hour:.2} ticks/hour > {cap:.2}")]
    PerScheduleHourlyCap {
        schedule: String,
        ticks_per_hour: f64,
        cap: f64,
    },
    #[error("schedule `{schedule}` exceeds provider `{provider}` daily density cap: {ticks_per_day:.2} ticks/day > {cap:.2}")]
    ProviderDailyCap {
        schedule: String,
        provider: ProviderName,
        ticks_per_day: f64,
        cap: f64,
    },
}

pub fn parse_cadence(expr: &str) -> Result<Cadence, ScheduleValidationError> {
    let normalized = expr.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "hourly" => {
            return Ok(Cadence {
                ticks_per_day: 24.0,
            })
        }
        "daily" => return Ok(Cadence { ticks_per_day: 1.0 }),
        "weekly" => {
            return Ok(Cadence {
                ticks_per_day: 1.0 / 7.0,
            })
        }
        _ => {}
    }

    if let Some(rest) = normalized.strip_prefix("every ") {
        let mut parts = rest.split_whitespace();
        let Some(n) = parts.next().and_then(|s| s.parse::<f64>().ok()) else {
            return Err(ScheduleValidationError::UnsupportedCadence(expr.into()));
        };
        let Some(unit) = parts.next() else {
            return Err(ScheduleValidationError::UnsupportedCadence(expr.into()));
        };
        if n <= 0.0 || parts.next().is_some() {
            return Err(ScheduleValidationError::UnsupportedCadence(expr.into()));
        }
        return match unit {
            "minute" | "minutes" => Ok(Cadence {
                ticks_per_day: 1_440.0 / n,
            }),
            "hour" | "hours" => Ok(Cadence {
                ticks_per_day: 24.0 / n,
            }),
            _ => Err(ScheduleValidationError::UnsupportedCadence(expr.into())),
        };
    }

    parse_simple_cron(&normalized)
        .ok_or_else(|| ScheduleValidationError::UnsupportedCadence(expr.into()))
}

pub fn validate_schedule_density(
    schedule: &ScheduleConfig,
    caps: DensityCaps,
) -> Result<Cadence, ScheduleValidationError> {
    let cadence = parse_cadence(&schedule.cron)?;
    let ticks_per_hour = cadence.ticks_per_day / 24.0;
    if ticks_per_hour > caps.max_ticks_per_hour_per_schedule {
        return Err(ScheduleValidationError::PerScheduleHourlyCap {
            schedule: schedule.name.clone(),
            ticks_per_hour,
            cap: caps.max_ticks_per_hour_per_schedule,
        });
    }
    for provider in &schedule.providers {
        if cadence.ticks_per_day > caps.max_ticks_per_day_per_provider {
            return Err(ScheduleValidationError::ProviderDailyCap {
                schedule: schedule.name.clone(),
                provider: *provider,
                ticks_per_day: cadence.ticks_per_day,
                cap: caps.max_ticks_per_day_per_provider,
            });
        }
    }
    Ok(cadence)
}

pub fn project_schedule_cost(
    schedule: &ScheduleConfig,
) -> Result<ScheduleProjection, ScheduleValidationError> {
    let cadence = validate_schedule_density(schedule, DensityCaps::default())?;
    let cost = project_monthly_cost(
        &schedule.providers,
        schedule.prompts.len(),
        cadence.ticks_per_day,
    );
    Ok(ScheduleProjection { cadence, cost })
}

pub fn validate_config_schedules(
    config: &Config,
) -> Result<Vec<ScheduleProjection>, ScheduleValidationError> {
    config.schedules.iter().map(project_schedule_cost).collect()
}

fn parse_simple_cron(expr: &str) -> Option<Cadence> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return None;
    }
    let minute_ticks = field_ticks(fields[0], 60.0)?;
    let hour_ticks = field_ticks(fields[1], 24.0)?;
    match (fields[2], fields[3], fields[4]) {
        ("*", "*", "*") => Some(Cadence {
            ticks_per_day: minute_ticks * hour_ticks,
        }),
        ("*", "*", dow) if dow.parse::<u32>().is_ok() => Some(Cadence {
            ticks_per_day: (minute_ticks * hour_ticks) / 7.0,
        }),
        _ => None,
    }
}

fn field_ticks(field: &str, range: f64) -> Option<f64> {
    if field == "*" {
        return Some(range);
    }
    if let Some(step) = field.strip_prefix("*/") {
        let n = step.parse::<f64>().ok()?;
        return (n > 0.0).then_some((range / n).ceil());
    }
    if field.parse::<u32>().is_ok() {
        return Some(1.0);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule(cron: &str) -> ScheduleConfig {
        ScheduleConfig {
            name: "daily-check".into(),
            cron: cron.into(),
            prompts: vec!["example-prompt".into()],
            providers: vec![ProviderName::Openai],
            debounce_minutes: 5,
            projection_acknowledged_at: None,
        }
    }

    #[test]
    fn parses_supported_shorthand() {
        assert_eq!(parse_cadence("hourly").unwrap().ticks_per_day, 24.0);
        assert_eq!(parse_cadence("daily").unwrap().ticks_per_day, 1.0);
        assert_eq!(parse_cadence("every 6 hours").unwrap().ticks_per_day, 4.0);
    }

    #[test]
    fn validates_density_cap_boundaries() {
        let caps = DensityCaps {
            max_ticks_per_hour_per_schedule: 12.0,
            max_ticks_per_day_per_provider: 96.0,
        };
        assert!(validate_schedule_density(&schedule("every 15 minutes"), caps).is_ok());
        assert!(matches!(
            validate_schedule_density(&schedule("every 5 minutes"), caps),
            Err(ScheduleValidationError::ProviderDailyCap { .. })
                | Err(ScheduleValidationError::PerScheduleHourlyCap { .. })
        ));
    }

    #[test]
    fn projects_monthly_cost() {
        let projection = project_schedule_cost(&schedule("daily")).unwrap();
        assert!(projection.cost.projected_monthly_usd > 0.0);
        assert!((projection.cost.runs_per_month - 30.4375).abs() < 0.01);
    }
}
