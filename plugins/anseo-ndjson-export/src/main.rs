//! `anseo-ndjson-export` — first-party reference **Output-format** plugin
//! (Story 41.5).
//!
//! ## What this binary actually does
//!
//! It is a real, fully-functional formatter that converts a completed run's
//! result rows into **NDJSON** (newline-delimited JSON) — the canonical shape
//! for streaming a run's output into a downstream sink (a file, `jq`, a
//! warehouse loader, etc.). It runs *entirely offline*: the only execution
//! primitive the host offers a plugin is the analytics-style subprocess, whose
//! sandbox (`crates/plugin-host/src/subprocess.rs`) denies `socket`/`connect`,
//! so a plugin **cannot stream over the network itself**. Instead it formats to
//! **stdout** and the host owns delivery.
//!
//! Contract (identical to the analytics subprocess primitive):
//!   * request arrives as **argv[1]** (no stdin under the sandbox),
//!   * formatted output is written to **stdout** (`RunOutcome::Exited`),
//!   * stderr is discarded by the host.
//!
//! ## Request (argv[1], JSON)
//!
//! ```json
//! { "run_id": "r-123", "rows": [ {"prompt":"p1","score":0.8}, {"prompt":"p2","score":0.4} ] }
//! ```
//!
//! ## Output (stdout, NDJSON — one compact JSON object per line)
//!
//! ```text
//! {"run_id":"r-123","prompt":"p1","score":0.8}
//! {"run_id":"r-123","prompt":"p2","score":0.4}
//! ```
//!
//! Each row is re-serialized through `serde_json`, so quoting/escaping is always
//! correct. No new MCP tool / Web route / CLI verb is introduced; output reaches
//! users through the existing output surface.

use std::process::ExitCode;

use serde_json::{json, Map, Value};

fn main() -> ExitCode {
    let request = std::env::args().nth(1).unwrap_or_default();
    match run(&request) {
        Ok(ndjson) => {
            // `ndjson` already ends each record with '\n'; print without an
            // extra trailing newline beyond what each line carries.
            print!("{ndjson}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            // stderr is discarded; emit a structured error on stdout.
            println!("{}", json!({ "error": message }));
            ExitCode::FAILURE
        }
    }
}

/// Format the request rows as NDJSON. Pure and offline — unit-tested below.
/// Each emitted line is `{ run_id, ...row-fields }`.
fn run(request: &str) -> Result<String, String> {
    if request.trim().is_empty() {
        return Err("empty request: expected JSON on argv[1]".to_string());
    }
    let req: Value = serde_json::from_str(request).map_err(|e| format!("invalid JSON: {e}"))?;

    let run_id = req
        .get("run_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let rows = req
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| "request must contain a `rows` array".to_string())?;

    if rows.is_empty() {
        return Err("`rows` must not be empty".to_string());
    }

    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        let obj = row
            .as_object()
            .ok_or_else(|| format!("row {i} is not a JSON object"))?;

        // Stamp run_id onto each row (run_id first for readable output), then
        // carry the row's own fields through verbatim.
        let mut merged = Map::new();
        merged.insert("run_id".to_string(), Value::String(run_id.clone()));
        for (k, v) in obj {
            if k == "run_id" {
                continue; // run_id is host-authoritative; ignore any row override
            }
            merged.insert(k.clone(), v.clone());
        }

        // Re-serialize so escaping is always correct.
        let line = serde_json::to_string(&Value::Object(merged))
            .map_err(|e| format!("row {i} failed to serialize: {e}"))?;
        out.push_str(&line);
        out.push('\n');
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rows_as_ndjson() {
        let out = run(
            r#"{"run_id":"r-123","rows":[{"prompt":"p1","score":0.8},{"prompt":"p2","score":0.4}]}"#,
        )
        .expect("run succeeds");

        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "one NDJSON line per row");

        let l0: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(l0["run_id"], "r-123");
        assert_eq!(l0["prompt"], "p1");
        assert_eq!(l0["score"], 0.8);

        let l1: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(l1["prompt"], "p2");
    }

    #[test]
    fn run_id_is_host_authoritative() {
        // A row trying to override run_id must not win.
        let out = run(r#"{"run_id":"real","rows":[{"run_id":"spoof","x":1}]}"#)
            .expect("run succeeds");
        let l0: Value = serde_json::from_str(out.lines().next().unwrap()).unwrap();
        assert_eq!(l0["run_id"], "real");
        assert_eq!(l0["x"], 1);
    }

    #[test]
    fn escaping_is_correct() {
        let out = run(r#"{"run_id":"r","rows":[{"prompt":"say \"hi\"\nbye"}]}"#)
            .expect("run succeeds");
        // Exactly one record line (the embedded \n is inside a JSON string, not a
        // record separator).
        let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1);
        let v: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["prompt"], "say \"hi\"\nbye");
    }

    #[test]
    fn empty_request_is_error() {
        assert!(run("").is_err());
    }

    #[test]
    fn missing_rows_is_error() {
        assert!(run(r#"{"run_id":"r"}"#).is_err());
    }

    #[test]
    fn non_object_row_is_error() {
        assert!(run(r#"{"run_id":"r","rows":[42]}"#).is_err());
    }
}
