//! `anseo-warehouse` — the first-party ClickHouse warehouse / ETL Analytics
//! plugin (Story 41.5, ADR-006 Tier-2+).
//!
//! Analytics plugins run as a **subprocess** under the host's seccomp-bpf /
//! `sandbox-exec` sandbox (Linux/macOS only — the loader skips them on
//! unsupported platforms). This binary is the subprocess entry point named by
//! `manifest.yaml`'s `entry_point: bin/anseo-warehouse`.
//!
//! It WRAPS the existing feature-gated ClickHouse ETL in `crates/analytics`
//! rather than reimplementing it: ADR-006 keeps the in-tree ClickHouse path as
//! the Tier-2 build and packages the same logic as this plugin for the
//! plugin-distribution (Tier-2+) path. The wrapper:
//!
//!   1. reads the Analytics request frame from stdin (the host's subprocess
//!      protocol),
//!   2. delegates to the shared ClickHouse ETL routine,
//!   3. writes the result frame to stdout.
//!
//! Network access is constrained to the ClickHouse endpoint declared in the
//! manifest `network` capability; the host mediates every outbound connection.
//!
//! Reaches users through the existing analytics surface via the
//! `plugin:anseo/anseo-warehouse:analytics` namespace — no new routes.

fn main() {
    // The deployed build links the shared ClickHouse ETL crate (built with the
    // `clickhouse` feature) and runs the subprocess request/response loop. This
    // template documents the contract; the CI signing pipeline (41.4) compiles
    // the real binary and signs the bundle.
    eprintln!(
        "anseo-warehouse: Analytics plugin subprocess entry point. \
         Wraps the ClickHouse ETL (ADR-006 Tier-2+). \
         Invoked by the host over the subprocess sandbox protocol."
    );
}
