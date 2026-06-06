//! `anseo-trend-analytics` — first-party reference **Analytics** plugin
//! (Story 41.5).
//!
//! ## What this binary actually does
//!
//! It is a real, fully-functional analytics computation that runs *entirely
//! offline* within the host's analytics-subprocess sandbox. The sandbox
//! (`crates/plugin-host/src/subprocess.rs`) denies `socket`/`connect`
//! (seccomp-bpf on Linux, `sandbox-exec (deny network*)` on macOS), so a plugin
//! **cannot reach the network**. The host's execution primitive is:
//!
//!   * the host spawns the program with **args** (no stdin is provided),
//!   * captures the program's **stdout** bytes (`RunOutcome::Exited { stdout }`),
//!   * **discards stderr**.
//!
//! So this plugin reads its request from `argv[1]` and writes its result to
//! **stdout**. It performs a genuine rollup over the supplied metric series —
//! count, min, max, mean, and a least-squares slope — and classifies the trend.
//! There are no stubs and nothing is logged-and-exited.
//!
//! ## Request (argv[1], JSON)
//!
//! ```json
//! { "window": "30d", "metric": "citation_share", "points": [0.10, 0.12, 0.15] }
//! ```
//!
//! ## Result (stdout, JSON)
//!
//! ```json
//! {
//!   "trend_kind": "plugin:anseo-trend-analytics:rollup",
//!   "metric": "citation_share",
//!   "window": "30d",
//!   "count": 3,
//!   "min": 0.10, "max": 0.15, "mean": 0.1233,
//!   "slope": 0.025,
//!   "direction": "rising"
//! }
//! ```
//!
//! `trend_kind` is namespaced `plugin:<name>:<kind>` per
//! `crates/plugin-manifest/src/trend_kind.rs`, so `list_trends` recognises it as
//! a plugin-emitted trend. No new MCP tool / Web route / CLI verb is introduced;
//! the result reaches users through the existing analytics surface.

use std::process::ExitCode;

use serde_json::{json, Value};

/// The namespaced trend kind this plugin emits (matches the convention in
/// `crates/plugin-manifest/src/trend_kind.rs`: `plugin:<name>:<kind>`).
const TREND_KIND: &str = "plugin:anseo-trend-analytics:rollup";

fn main() -> ExitCode {
    // The host passes the request as argv[1]; no stdin is available under the
    // sandbox. Stderr is discarded by the host, so all diagnostics go to stdout
    // as a structured error result.
    let request = std::env::args().nth(1).unwrap_or_default();
    match run(&request) {
        Ok(result) => {
            println!("{result}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            // Emit a structured, machine-readable error on stdout (stderr is
            // discarded). Non-zero exit so the host marks the run failed.
            println!("{}", json!({ "error": message }));
            ExitCode::FAILURE
        }
    }
}

/// Parse the request, compute the rollup, and return the result JSON as a
/// compact string. Pure and offline — unit-tested below.
fn run(request: &str) -> Result<String, String> {
    if request.trim().is_empty() {
        return Err("empty request: expected JSON on argv[1]".to_string());
    }
    let req: Value = serde_json::from_str(request).map_err(|e| format!("invalid JSON: {e}"))?;

    let metric = req
        .get("metric")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let window = req
        .get("window")
        .and_then(Value::as_str)
        .unwrap_or("unspecified")
        .to_string();

    let points: Vec<f64> = req
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| "request must contain a `points` array".to_string())?
        .iter()
        .map(|v| {
            v.as_f64()
                .ok_or_else(|| format!("non-numeric point: {v}"))
        })
        .collect::<Result<_, _>>()?;

    if points.is_empty() {
        return Err("`points` must not be empty".to_string());
    }

    let rollup = Rollup::compute(&points);

    Ok(json!({
        "trend_kind": TREND_KIND,
        "metric": metric,
        "window": window,
        "count": rollup.count,
        "min": round4(rollup.min),
        "max": round4(rollup.max),
        "mean": round4(rollup.mean),
        "slope": round4(rollup.slope),
        "direction": rollup.direction(),
    })
    .to_string())
}

/// A genuine statistical rollup over an evenly-spaced metric series.
struct Rollup {
    count: usize,
    min: f64,
    max: f64,
    mean: f64,
    /// Least-squares slope over index 0..n (per-step change).
    slope: f64,
}

impl Rollup {
    fn compute(points: &[f64]) -> Rollup {
        let count = points.len();
        let n = count as f64;
        let mean = points.iter().sum::<f64>() / n;
        let min = points.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = points.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        // Least-squares slope of value vs. index. x = 0..n-1.
        let x_mean = (n - 1.0) / 2.0;
        let mut num = 0.0;
        let mut den = 0.0;
        for (i, &y) in points.iter().enumerate() {
            let dx = i as f64 - x_mean;
            num += dx * (y - mean);
            den += dx * dx;
        }
        let slope = if den == 0.0 { 0.0 } else { num / den };

        Rollup {
            count,
            min,
            max,
            mean,
            slope,
        }
    }

    /// Classify the trend. The epsilon keeps near-flat series from reading as
    /// noise-driven movement.
    fn direction(&self) -> &'static str {
        const EPS: f64 = 1e-9;
        if self.slope > EPS {
            "rising"
        } else if self.slope < -EPS {
            "falling"
        } else {
            "flat"
        }
    }
}

/// Round to 4 decimal places so the emitted JSON is stable and readable.
fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rising_series_classifies_rising() {
        let out = run(r#"{"window":"30d","metric":"citation_share","points":[0.10,0.12,0.15]}"#)
            .expect("run succeeds");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["trend_kind"], TREND_KIND);
        assert_eq!(v["metric"], "citation_share");
        assert_eq!(v["window"], "30d");
        assert_eq!(v["count"], 3);
        assert_eq!(v["min"], 0.10);
        assert_eq!(v["max"], 0.15);
        assert_eq!(v["direction"], "rising");
        assert!(v["slope"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn falling_series_classifies_falling() {
        let out = run(r#"{"points":[3.0,2.0,1.0]}"#).expect("run succeeds");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["direction"], "falling");
        assert!(v["slope"].as_f64().unwrap() < 0.0);
    }

    #[test]
    fn flat_series_classifies_flat() {
        let out = run(r#"{"points":[5.0,5.0,5.0]}"#).expect("run succeeds");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["direction"], "flat");
        assert_eq!(v["slope"], 0.0);
    }

    #[test]
    fn empty_request_is_error() {
        assert!(run("").is_err());
        assert!(run("   ").is_err());
    }

    #[test]
    fn missing_points_is_error() {
        assert!(run(r#"{"metric":"x"}"#).is_err());
    }

    #[test]
    fn non_numeric_point_is_error() {
        assert!(run(r#"{"points":["nope"]}"#).is_err());
    }
}
