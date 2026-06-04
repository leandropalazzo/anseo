//! Schedule declaration validation + background-worker substrate for Phase 2.

pub mod dispatch;
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

/// A parsed schedule recurrence. Two families:
///
/// - [`Recurrence::Frequency`] — the legacy shorthands (`hourly`, `daily`,
///   `every N hours`, simple 5-field cron). These fire on epoch-aligned UTC
///   interval boundaries; they carry no wall-clock time-of-day or weekday.
/// - [`Recurrence::Calendar`] — Google-Calendar-style recurrence with an
///   explicit timezone, time-of-day, and either every-day / specific-weekdays /
///   every-N-days. Fires at the next matching wall-clock instant in `tz`.
///
/// The wire form lives in the schedule's `cron` string. Calendar recurrences
/// are encoded as `TZ=<iana> daily at HH:MM`, `TZ=<iana> weekly on mon,wed,fri
/// at HH:MM`, or `TZ=<iana> every N days at HH:MM`. Anything without a `TZ=`
/// prefix is parsed as a legacy frequency, so existing schedules keep working.
#[derive(Debug, Clone, PartialEq)]
pub enum Recurrence {
    Frequency(Cadence),
    Calendar(CalendarSpec),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalendarSpec {
    /// IANA timezone name (e.g. `America/New_York`). Validated at next-tick time.
    pub tz: String,
    /// Hour of day, 0–23, in `tz`.
    pub hour: u32,
    /// Minute of hour, 0–59.
    pub minute: u32,
    pub cadence: CalendarCadence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalendarCadence {
    /// Every day at `hour:minute`.
    Daily,
    /// On the listed weekdays (0=Sun .. 6=Sat), sorted + deduped, at `hour:minute`.
    Weekly(Vec<u32>),
    /// Every N days at `hour:minute`, anchored to the schedule's last run.
    EveryNDays(u32),
}

impl Recurrence {
    /// Average runs-per-day, used for density caps + monthly cost projection.
    pub fn cadence(&self) -> Cadence {
        match self {
            Recurrence::Frequency(c) => *c,
            Recurrence::Calendar(spec) => Cadence {
                ticks_per_day: match &spec.cadence {
                    CalendarCadence::Daily => 1.0,
                    CalendarCadence::Weekly(days) => (days.len().max(1) as f64) / 7.0,
                    CalendarCadence::EveryNDays(n) => 1.0 / (*n).max(1) as f64,
                },
            },
        }
    }
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
    Ok(parse_recurrence(expr)?.cadence())
}

/// Parse a schedule `cron` string into a [`Recurrence`]. A `TZ=` prefix selects
/// the calendar grammar; everything else falls through to the legacy
/// frequency shorthands + simple cron.
pub fn parse_recurrence(expr: &str) -> Result<Recurrence, ScheduleValidationError> {
    let trimmed = expr.trim();
    // Case-insensitive `TZ=` prefix marks a calendar recurrence. The timezone
    // value is case-sensitive (IANA names like `America/New_York`), so parse it
    // off the raw (non-lowercased) string.
    if trimmed.len() >= 3 && trimmed[..3].eq_ignore_ascii_case("tz=") {
        return parse_calendar(trimmed);
    }
    parse_frequency(expr).map(Recurrence::Frequency)
}

/// Parse `TZ=<iana> {daily | weekly on <days> | every N days} at HH:MM`.
fn parse_calendar(expr: &str) -> Result<Recurrence, ScheduleValidationError> {
    let unsupported = || ScheduleValidationError::UnsupportedCadence(expr.into());

    let mut tokens = expr.split_whitespace();
    let tz_token = tokens.next().ok_or_else(unsupported)?;
    let tz = tz_token
        .get(3..)
        .filter(|s| !s.is_empty())
        .ok_or_else(unsupported)?
        .to_string();

    // The remainder, lowercased for keyword matching. The time + day names are
    // ASCII so this is safe.
    let rest: Vec<String> = tokens.map(|t| t.to_ascii_lowercase()).collect();
    if rest.len() < 3 {
        return Err(unsupported());
    }
    // Last two tokens are always `at HH:MM`.
    let at_idx = rest.len() - 2;
    if rest[at_idx] != "at" {
        return Err(unsupported());
    }
    let (hour, minute) = parse_hh_mm(&rest[at_idx + 1]).ok_or_else(unsupported)?;
    let head = &rest[..at_idx];

    let cadence = match head[0].as_str() {
        "daily" if head.len() == 1 => CalendarCadence::Daily,
        "weekly" if head.len() == 3 && head[1] == "on" => {
            let days = parse_weekdays(&head[2]).ok_or_else(unsupported)?;
            CalendarCadence::Weekly(days)
        }
        "every" if head.len() == 3 && head[2] == "days" => {
            let n: u32 = head[1]
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(unsupported)?;
            CalendarCadence::EveryNDays(n)
        }
        _ => return Err(unsupported()),
    };

    Ok(Recurrence::Calendar(CalendarSpec {
        tz,
        hour,
        minute,
        cadence,
    }))
}

fn parse_hh_mm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    let hour: u32 = h.parse().ok()?;
    let minute: u32 = m.parse().ok()?;
    (hour < 24 && minute < 60).then_some((hour, minute))
}

/// Parse a comma-separated weekday list (`mon,wed,fri`) into sorted, deduped
/// indices (0=Sun .. 6=Sat). Returns `None` on any unknown token or empty list.
fn parse_weekdays(s: &str) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    for token in s.split(',') {
        let idx = match token.trim() {
            "sun" => 0,
            "mon" => 1,
            "tue" => 2,
            "wed" => 3,
            "thu" => 4,
            "fri" => 5,
            "sat" => 6,
            _ => return None,
        };
        if !out.contains(&idx) {
            out.push(idx);
        }
    }
    if out.is_empty() {
        return None;
    }
    out.sort_unstable();
    Some(out)
}

fn parse_frequency(expr: &str) -> Result<Cadence, ScheduleValidationError> {
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
                provider: provider.clone(),
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

    #[test]
    fn parses_calendar_daily() {
        let rec = parse_recurrence("TZ=America/New_York daily at 09:30").unwrap();
        assert_eq!(
            rec,
            Recurrence::Calendar(CalendarSpec {
                tz: "America/New_York".into(),
                hour: 9,
                minute: 30,
                cadence: CalendarCadence::Daily,
            })
        );
        assert_eq!(rec.cadence().ticks_per_day, 1.0);
    }

    #[test]
    fn parses_calendar_weekly_sorted_deduped() {
        let rec = parse_recurrence("TZ=Europe/London weekly on fri,mon,mon at 14:00").unwrap();
        assert_eq!(
            rec,
            Recurrence::Calendar(CalendarSpec {
                tz: "Europe/London".into(),
                hour: 14,
                minute: 0,
                cadence: CalendarCadence::Weekly(vec![1, 5]),
            })
        );
        assert!((rec.cadence().ticks_per_day - 2.0 / 7.0).abs() < 1e-9);
    }

    #[test]
    fn parses_calendar_every_n_days() {
        let rec = parse_recurrence("TZ=UTC every 3 days at 00:15").unwrap();
        assert_eq!(
            rec,
            Recurrence::Calendar(CalendarSpec {
                tz: "UTC".into(),
                hour: 0,
                minute: 15,
                cadence: CalendarCadence::EveryNDays(3),
            })
        );
        assert!((rec.cadence().ticks_per_day - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_malformed_calendar() {
        assert!(parse_recurrence("TZ= daily at 09:30").is_err());
        assert!(parse_recurrence("TZ=UTC daily at 25:00").is_err());
        assert!(parse_recurrence("TZ=UTC weekly on funday at 09:30").is_err());
        assert!(parse_recurrence("TZ=UTC every 0 days at 09:30").is_err());
        assert!(parse_recurrence("TZ=UTC daily").is_err());
    }

    #[test]
    fn legacy_shorthands_still_parse_as_frequency() {
        assert!(matches!(
            parse_recurrence("daily").unwrap(),
            Recurrence::Frequency(_)
        ));
        assert!(matches!(
            parse_recurrence("every 6 hours").unwrap(),
            Recurrence::Frequency(_)
        ));
    }
}
