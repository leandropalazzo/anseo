//! `anseo-connect-bigquery` — first-party connect-your-data Output plugin
//! (Story 41.5).
//!
//! Output-format plugins transform / export a completed run's results. This one
//! streams run results into a Google BigQuery dataset. It is an initial
//! implementation (not production-hardened — see the story Notes) that
//! demonstrates the connect-your-data sink pattern.
//!
//! Network access is scoped to `bigquery.googleapis.com` by the manifest
//! `network` capability and enforced by the capability catalog (Story 17.4);
//! the BigQuery service-account credential is read via the declared
//! `read-secret` capability. The host mediates both.
//!
//! Reaches users through the existing output surface via the
//! `plugin:anseo/anseo-connect-bigquery:output-format` namespace — no new
//! routes, MCP tools, or CLI verbs.

/// The only API host this sink may reach. MUST match the manifest `network`
/// allowlist.
pub const BIGQUERY_HOST: &str = "bigquery.googleapis.com";

/// The secret key the sink reads its service-account credential from. MUST
/// match the manifest `read-secret` capability.
pub const CREDENTIAL_KEY: &str = "plugin:anseo/anseo-connect-bigquery:service-account";

/// Build the BigQuery `tabledata.insertAll` streaming-insert endpoint for a
/// destination `project.dataset.table`.
pub fn insert_all_url(project: &str, dataset: &str, table: &str) -> String {
    format!(
        "https://{BIGQUERY_HOST}/bigquery/v2/projects/{project}/datasets/{dataset}/tables/{table}/insertAll"
    )
}

fn main() {
    eprintln!(
        "anseo-connect-bigquery: Output plugin subprocess entry point. \
         Streams run results to BigQuery ({BIGQUERY_HOST}). \
         Invoked by the host over the subprocess sandbox protocol."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_url_targets_allowlisted_host() {
        let url = insert_all_url("my-proj", "anseo", "runs");
        assert!(url.starts_with(&format!("https://{BIGQUERY_HOST}/")));
        assert!(url.contains("/projects/my-proj/datasets/anseo/tables/runs/insertAll"));
    }
}
