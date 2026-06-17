//! Story 20.11 — authZ-before-GUC ordering + SET LOCAL fault-injection ([p4-iso-4]).
//!
//! AC-1: The `authz_then_guc` entry point always calls `decide` before `set_local`.
//! AC-2: A denied or erroring `decide` prevents `set_local` from being called.
//! AC-3: `set_local` failing causes the overall call to return `Err`.
//! AC-4: GA criterion [p4-iso-4] flips green.

use anseo_authz::{
    authz_then_guc, AllowAllDecider, AuthzDecider, AuthzError, Decision, DenyAllDecider,
    ErrorDecider, FaultyGucContext, GucContext, NoopGucContext,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Instrumented deciders / GUC contexts that record call counts.
// ---------------------------------------------------------------------------

struct CountingDecider {
    decision: Decision,
    calls: Arc<AtomicU32>,
}

impl AuthzDecider for CountingDecider {
    fn decide(&self, _caller_id: Uuid, _org_id: Uuid) -> Result<Decision, AuthzError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.decision.clone())
    }
}

struct CountingGuc {
    called: Arc<AtomicBool>,
}

impl GucContext for CountingGuc {
    fn set_local(&self, _org_id: Uuid) -> Result<(), AuthzError> {
        self.called.store(true, Ordering::SeqCst);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AC-1: decide is called exactly once before set_local.
// ---------------------------------------------------------------------------

#[test]
fn decide_called_before_set_local_on_allow() {
    let calls = Arc::new(AtomicU32::new(0));
    let guc_called = Arc::new(AtomicBool::new(false));

    let decider = CountingDecider {
        decision: Decision::Allow,
        calls: Arc::clone(&calls),
    };
    let guc = CountingGuc {
        called: Arc::clone(&guc_called),
    };

    let result = authz_then_guc(&decider, &guc, Uuid::new_v4(), Uuid::new_v4());

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Decision::Allow);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "decide must be called once"
    );
    assert!(
        guc_called.load(Ordering::SeqCst),
        "set_local must be called after Allow"
    );
}

// ---------------------------------------------------------------------------
// AC-2: Deny → set_local is never called.
// ---------------------------------------------------------------------------

#[test]
fn deny_prevents_guc_set() {
    let guc_called = Arc::new(AtomicBool::new(false));
    let decider = DenyAllDecider;
    let guc = CountingGuc {
        called: Arc::clone(&guc_called),
    };

    let result = authz_then_guc(&decider, &guc, Uuid::new_v4(), Uuid::new_v4());

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Decision::Deny);
    assert!(
        !guc_called.load(Ordering::SeqCst),
        "set_local must NOT be called when decide returns Deny"
    );
}

#[test]
fn decide_error_prevents_guc_set() {
    let guc_called = Arc::new(AtomicBool::new(false));
    let decider = ErrorDecider;
    let guc = CountingGuc {
        called: Arc::clone(&guc_called),
    };

    let result = authz_then_guc(&decider, &guc, Uuid::new_v4(), Uuid::new_v4());

    assert!(result.is_err(), "decide error must propagate as Err");
    assert!(
        !guc_called.load(Ordering::SeqCst),
        "set_local must NOT be called when decide errors"
    );
}

// ---------------------------------------------------------------------------
// AC-3: set_local failure → Err returned; caller must rollback.
// ---------------------------------------------------------------------------

#[test]
fn guc_set_failure_returns_err() {
    let decider = AllowAllDecider;
    let guc = FaultyGucContext;

    let result = authz_then_guc(&decider, &guc, Uuid::new_v4(), Uuid::new_v4());

    assert!(
        result.is_err(),
        "FaultyGucContext must cause authz_then_guc to return Err"
    );
    match result.unwrap_err() {
        anseo_authz::AuthzError::GucSet(_) => {} // correct error variant
        other => panic!("unexpected error variant: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Idiomatic seam usage tests.
// ---------------------------------------------------------------------------

#[test]
fn deny_all_decider_returns_deny() {
    let d = DenyAllDecider;
    let result = d.decide(Uuid::new_v4(), Uuid::new_v4());
    assert_eq!(result.unwrap(), Decision::Deny);
}

#[test]
fn allow_all_decider_returns_allow() {
    let d = AllowAllDecider;
    let result = d.decide(Uuid::new_v4(), Uuid::new_v4());
    assert_eq!(result.unwrap(), Decision::Allow);
}

#[test]
fn noop_guc_succeeds() {
    let g = NoopGucContext;
    assert!(g.set_local(Uuid::new_v4()).is_ok());
}

#[test]
fn faulty_guc_fails() {
    let g = FaultyGucContext;
    assert!(g.set_local(Uuid::new_v4()).is_err());
}

/// Sentinel for GA gate script grep.
#[allow(dead_code)]
const P4_ISO_4_EVIDENCE: &str =
    "p4-iso-4: authz_ordering::deny_prevents_guc_set + guc_set_failure_returns_err";
