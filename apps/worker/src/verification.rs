//! Story 43.2 (AC-5) — worker side of the daily DNS re-verification job.
//!
//! The job logic itself lives in `anseo-storage`
//! (`anseo_storage::repositories::verification::run_reverification_job`) so it
//! can be unit-tested with a mock resolver and no network. This module supplies
//! the **production** DNS resolver (`HickoryTxtResolver`) and the daily-cadence
//! gate the worker poll loop uses to drive it at most once per day.
//!
//! Cadence: the poll loop ticks every few seconds (see
//! [`crate::run::POLL_INTERVAL_SECONDS`]); we guard the re-verification sweep
//! behind a "last run" instant so it fires roughly once every 24h regardless of
//! the underlying poll frequency — a simple in-process timer, no extra infra.

use std::time::Duration;

use anseo_storage::repositories::verification::{ResolveError, TxtResolver};

/// Minimum interval between re-verification sweeps (AC-5: daily cadence).
pub const REVERIFY_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// A real async DNS TXT resolver backed by `hickory-resolver`, used by the
/// daily re-verification job to confirm a verified domain's challenge TXT
/// record is still published. Mirrors the API crate's resolver so production
/// behaviour is identical; the storage job is parameterised on the
/// [`TxtResolver`] trait so tests inject the in-memory mock instead.
pub struct HickoryTxtResolver {
    resolver: hickory_resolver::TokioAsyncResolver,
}

impl HickoryTxtResolver {
    /// Build a resolver from the host's `/etc/resolv.conf`, falling back to a
    /// default (Google/Cloudflare) config when the system config is unreadable
    /// (e.g. minimal containers).
    pub fn new() -> Self {
        use hickory_resolver::config::{ResolverConfig, ResolverOpts};
        use hickory_resolver::TokioAsyncResolver;
        let resolver = match TokioAsyncResolver::tokio_from_system_conf() {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    event = "verification.resolver_system_conf_failed",
                    error = %e,
                    "falling back to default DNS resolver config"
                );
                TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default())
            }
        };
        Self { resolver }
    }
}

impl Default for HickoryTxtResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TxtResolver for HickoryTxtResolver {
    async fn lookup_txt(&self, name: &str) -> Result<Vec<String>, ResolveError> {
        use hickory_resolver::error::ResolveErrorKind;
        match self.resolver.txt_lookup(name).await {
            Ok(lookup) => {
                // Each TXT record may be split into multiple character-strings;
                // concatenate them per record (DNS semantics) into one value.
                let values = lookup
                    .iter()
                    .map(|txt| {
                        txt.iter()
                            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
                            .collect::<String>()
                    })
                    .collect();
                Ok(values)
            }
            Err(e) => match e.kind() {
                // No records / NXDOMAIN → treat as "absent" (drives revocation),
                // NOT a transient failure.
                ResolveErrorKind::NoRecordsFound { .. } => {
                    Err(ResolveError::NotFound(name.to_string()))
                }
                _ => Err(ResolveError::Transient(e.to_string())),
            },
        }
    }
}
