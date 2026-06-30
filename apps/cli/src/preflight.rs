//! Preflight identity probe (Story 37.8 stub — Story 37.9 fills in the real check).
//!
//! Story 37.9 will validate the DB sentinel UUID, schema version, and instance
//! identity before the wizard hands off to the user. Until then this is a no-op
//! so the `anseo init` bring-up pipeline compiles end-to-end.

use anseo_core::OpenGeoError;

/// Run pre-handoff sanity checks after the tier backend is started.
///
/// Currently a no-op stub. Story 37.9 replaces this with:
/// - DB connectivity check
/// - Sentinel UUID probe (create on first run, verify on re-run)
/// - Schema version assertion
pub fn run_preflight() -> Result<(), OpenGeoError> {
    Ok(())
}
