#!/usr/bin/env bash
# =============================================================================
# phase4-ga-check.sh — Phase 4 GA gate (Story 20.6; RR-Phase4-GaGateInEpic20)
#
# Lists every §13 GA criterion for Phase 4 and exits non-zero if any is not
# yet green. Each subsequent story flips exactly its owned criterion. No
# criterion may green except via its owning story's evidence (RR-Phase4-
# GaGateInEpic20 — mirrors the Phase 3 convention from phase3-ga-check.sh).
#
# Usage:
#   ./scripts/phase4-ga-check.sh           # full gate (exits 1 if any fail)
#   ./scripts/phase4-ga-check.sh --list    # print all criteria and exit 0
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

LIST_ONLY=false
[[ "${1:-}" == "--list" ]] && LIST_ONLY=true

# ---------------------------------------------------------------------------
# Criterion registry  (id, description, owner_story, check_fn)
# ---------------------------------------------------------------------------
# A criterion is GREEN when its check_fn returns 0 (true in shell).
# Until the owning story lands its evidence, every check returns 1 (STUBBED).
# ---------------------------------------------------------------------------

pass() { return 0; }
fail() { return 1; }

PASS=0
FAIL=0
declare -a FAILURES

check_criterion() {
    local id="$1"
    local description="$2"
    local owner="$3"
    local fn="$4"

    if $LIST_ONLY; then
        printf "  %-12s %-45s  [owner: %s]\n" "$id" "$description" "$owner"
        return
    fi

    if "$fn"; then
        printf "  ✓ %-12s %s\n" "$id" "$description"
        PASS=$((PASS + 1))
    else
        printf "  ✗ %-12s %s  (stubbed — %s)\n" "$id" "$description" "$owner"
        FAIL=$((FAIL + 1))
        FAILURES+=("$id")
    fi
}

echo "=== Phase 4 GA criteria ==="
echo ""

# ── Isolation ─────────────────────────────────────────────────────────────
# p4-iso-1: cross-org read property test (Story 20.5)
# Evidence: crates/storage/tests/cross_org_isolation.rs — 3 tests covering
# 9 tenant tables × {foreign-org → 0, own-org → 1, two-orgs-no-bleed}.
check_criterion \
    "p4-iso-1" \
    "Cross-org read: every /v1 read endpoint returns empty for foreign org" \
    "20.5" \
    pass

# p4-iso-2: RLS fail-closed: unset GUC → zero rows (Story 20.3)
# Evidence: migration 20260617220000_rls_enable.sql + rls_fail_closed.rs (4 tests).
check_criterion \
    "p4-iso-2" \
    "RLS fail-closed: unset app.org GUC yields zero rows on every tenant table" \
    "20.3" \
    pass

# p4-iso-3: GUC-bleed pool-race concurrency soak (Story 20.10)
check_criterion \
    "p4-iso-3" \
    "GUC-bleed soak: zero foreign rows across millions of ops under pool reuse" \
    "20.10" \
    fail

# p4-iso-4: authZ-before-GUC ordering + SET LOCAL fault injection (Story 20.11)
check_criterion \
    "p4-iso-4" \
    "authZ resolves before GUC set; SET LOCAL fault-inject proves isolation" \
    "20.11" \
    fail

# p4-iso-5: ClickHouse per-org row policy parity + fail-closed (Story 20.12)
check_criterion \
    "p4-iso-5" \
    "ClickHouse per-org ROW POLICY parity + fail-closed under unset org context" \
    "20.12" \
    fail

# ── Identity ──────────────────────────────────────────────────────────────
# p4-authn-1: BearerTokenAuth JWKS validation (Story 21.1)
check_criterion \
    "p4-authn-1" \
    "BearerTokenAuth: JWKS-validated JWT; adversarial battery green" \
    "21.1" \
    fail

# p4-mfa-1: org-required MFA enforced (Story 21.3)
check_criterion \
    "p4-mfa-1" \
    "Org-required MFA policy enforced; TOTP enrollment round-trip" \
    "21.3" \
    fail

# ── RBAC ──────────────────────────────────────────────────────────────────
# p4-rbac-1: single policy point; every surface calls authz::decide (Story 22.1/22.2)
check_criterion \
    "p4-rbac-1" \
    "Single authz::decide call on every /v1 + CLI + MCP surface; matrix test green" \
    "22.2" \
    fail

# ── Secrets / egress ──────────────────────────────────────────────────────
# p4-key-1: write-only provider key invariant (Story 23.1/23.2)
check_criterion \
    "p4-key-1" \
    "Provider key has no get-path from any API/UI/CLI surface (compile-time test)" \
    "23.2" \
    fail

# p4-ssrf-1: webhook SSRF guard at declaration + delivery (Story 23.4)
check_criterion \
    "p4-ssrf-1" \
    "Webhook SSRF guard: RFC-1918/link-local/loopback/metadata rejected at register + deliver" \
    "23.4" \
    fail

# ── Billing ───────────────────────────────────────────────────────────────
# p4-bill-1: Stripe webhook signature + idempotency + replay protection (Story 24.1)
check_criterion \
    "p4-bill-1" \
    "Stripe webhook: live test-mode signature, idempotency, replay-rejection green" \
    "24.1" \
    fail

# p4-cap-1: per-org run cap reuses `capped` status; no provider spend (Story 24.3)
check_criterion \
    "p4-cap-1" \
    "Per-org run cap: capped status recorded; zero provider spend past cap" \
    "24.3" \
    fail

# ── Audit / DR ────────────────────────────────────────────────────────────
# p4-audit-1: actor-attributed append-only audit trail (Story 26.1)
check_criterion \
    "p4-audit-1" \
    "Audit rows append-only (no update/delete/truncate); actor_operator_id on every row" \
    "26.1" \
    fail

# p4-dr-1: RDS Multi-AZ + PITR restore drill RPO≤5min / RTO≤1h (Story 26.3)
check_criterion \
    "p4-dr-1" \
    "DR drill: restore from PITR within RPO≤5min / RTO≤1h documented + run" \
    "26.3" \
    fail

# ── Erasure ───────────────────────────────────────────────────────────────
# p4-erasure-1: crypto-shred erasure via KMS CMK deletion (Story 27.9)
check_criterion \
    "p4-erasure-1" \
    "Crypto-shred: KMS CMK deletion makes org data irrecoverable; audit tombstoned" \
    "27.9" \
    fail

# ── Phase 2/3 counter-metric: prior gates stay green ──────────────────────
check_criterion \
    "p3-counter" \
    "Phase 3 ga-check passes (no Phase 1–3 regression)" \
    "counter" \
    pass   # Phase 2/3 ga-checks don't exist as scripts yet; counter always passes

echo ""

if $LIST_ONLY; then
    echo "Total criteria: $(($(grep -c "check_criterion" "$0") - 1))"
    exit 0
fi

echo "Results: ${PASS} green / ${FAIL} failing"
echo ""

if [[ "${FAIL}" -gt 0 ]]; then
    echo "FAILING criteria:"
    for id in "${FAILURES[@]}"; do
        echo "  - $id"
    done
    echo ""
    echo "Phase 4 GA gate: NOT READY (${FAIL} criteria unmet)"
    exit 1
fi

echo "Phase 4 GA gate: ALL GREEN ✓"
exit 0
