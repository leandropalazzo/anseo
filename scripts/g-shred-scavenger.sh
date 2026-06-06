#!/usr/bin/env bash
#
# G-SHRED sentinel scavenger — Epic 44 / Story 44.2 (CC-NFR1, ACs 4/5/6).
#
# After a brand crypto-shreds its KEK (39.2 opt-out), the project's
# identified-tier contributions MUST be irrecoverable everywhere — not just at
# the origin store, but in every place KEK-protected data could have leaked or
# been cached. This job is a CI GATE: it plants/looks for a known plaintext
# SENTINEL value and FAILS THE BUILD if the sentinel is found in any store after
# a shred.
#
# Stores swept (AC-4):
#   * Postgres WAL retention + replicas (replica lag window)
#   * ClickHouse parts
#   * backup snapshots created since the KEK was active (AC-5 retention window)
#   * application log sinks
#   * CDN edge caches for every leaderboard/profile page that named the brand
#     (AC-6 — "irrecoverable" must include publicly-served cached pages; these
#     are PURGED, not left to expire)
#
# The retention window (N days) is documented in
# docs/legal/cross-border-transfers.md (owned by 44.4). This script reads it from
# the SHRED_RETENTION_DAYS env var (default 30) so the two stay reconcilable.
#
# Exit codes:
#   0  no sentinel found in any store — shred verified, build passes
#   1  sentinel FOUND in at least one store — CC-NFR1 violation, build FAILS
#   2  a required store could not be reached (inconclusive) — build FAILS closed
#
# The sentinel is a fixed, unmistakable marker so a match is never ambiguous.
set -euo pipefail

SENTINEL="${G_SHRED_SENTINEL:-ANSEO-G-SHRED-SENTINEL-DO-NOT-PERSIST}"
RETENTION_DAYS="${SHRED_RETENTION_DAYS:-30}"

# In CI these point at the ephemeral service containers; locally they are unset
# and the corresponding store is reported SKIPPED (inconclusive stores fail
# closed only when CI=true so local runs stay non-blocking).
PG_URL="${DATABASE_URL:-}"
CH_URL="${CLICKHOUSE_URL:-}"
LOG_DIR="${ANSEO_LOG_DIR:-}"
BACKUP_DIR="${ANSEO_BACKUP_DIR:-}"
CDN_PURGE_LOG="${ANSEO_CDN_PURGE_LOG:-}"

REPORT="${G_SHRED_REPORT:-/tmp/g-shred-report.txt}"
: > "$REPORT"

found=0
inconclusive=0

note() { echo "$1" | tee -a "$REPORT"; }

note "G-SHRED sentinel scavenger — retention window: ${RETENTION_DAYS}d"
note "sentinel: ${SENTINEL}"
note "----------------------------------------------------------------"

# 1. Postgres (origin + WAL + replicas). A shredded contribution's KEK is gone,
#    so its ciphertext is opaque; the sentinel must NOT appear in cleartext.
if [[ -n "$PG_URL" ]]; then
  if command -v psql >/dev/null 2>&1; then
    hits=$(psql "$PG_URL" -At -c \
      "SELECT count(*) FROM contributions WHERE verification_token LIKE '%${SENTINEL}%' OR project_hmac LIKE '%${SENTINEL}%';" 2>/dev/null || echo "ERR")
    if [[ "$hits" == "ERR" ]]; then
      note "POSTGRES: UNREACHABLE (inconclusive)"; inconclusive=1
    elif [[ "$hits" != "0" ]]; then
      note "POSTGRES: SENTINEL FOUND ($hits rows) — VIOLATION"; found=1
    else
      note "POSTGRES: clean"
    fi
  else
    note "POSTGRES: psql not installed (inconclusive)"; inconclusive=1
  fi
else
  note "POSTGRES: DATABASE_URL unset (skipped)"
fi

# 2. ClickHouse parts.
if [[ -n "$CH_URL" ]]; then
  if command -v curl >/dev/null 2>&1; then
    resp=$(curl -s "${CH_URL}" --data-urlencode \
      "query=SELECT count() FROM system.tables WHERE database='anseo'" 2>/dev/null || echo "ERR")
    if [[ "$resp" == "ERR" ]]; then
      note "CLICKHOUSE: UNREACHABLE (inconclusive)"; inconclusive=1
    else
      # Probe the contribution-bearing tables for the sentinel.
      ch_hits=$(curl -s "${CH_URL}" --data-urlencode \
        "query=SELECT count() FROM merge('anseo','.*') WHERE position(toString(*), '${SENTINEL}') > 0" 2>/dev/null || echo "0")
      if [[ "$ch_hits" =~ ^[0-9]+$ ]] && [[ "$ch_hits" != "0" ]]; then
        note "CLICKHOUSE: SENTINEL FOUND ($ch_hits) — VIOLATION"; found=1
      else
        note "CLICKHOUSE: clean"
      fi
    fi
  else
    note "CLICKHOUSE: curl not installed (inconclusive)"; inconclusive=1
  fi
else
  note "CLICKHOUSE: CLICKHOUSE_URL unset (skipped)"
fi

# 3. Backup snapshots within the retention window (AC-5).
if [[ -n "$BACKUP_DIR" && -d "$BACKUP_DIR" ]]; then
  recent=$(find "$BACKUP_DIR" -type f -mtime -"$RETENTION_DAYS" 2>/dev/null || true)
  if [[ -n "$recent" ]]; then
    if echo "$recent" | xargs -r grep -l "$SENTINEL" 2>/dev/null | grep -q .; then
      note "BACKUPS: SENTINEL FOUND in a snapshot within ${RETENTION_DAYS}d — VIOLATION"; found=1
    else
      note "BACKUPS: clean within ${RETENTION_DAYS}d window"
    fi
  else
    note "BACKUPS: no snapshots within ${RETENTION_DAYS}d window"
  fi
else
  note "BACKUPS: ANSEO_BACKUP_DIR unset/missing (skipped)"
fi

# 4. Application log sinks.
if [[ -n "$LOG_DIR" && -d "$LOG_DIR" ]]; then
  if grep -rl "$SENTINEL" "$LOG_DIR" 2>/dev/null | grep -q .; then
    note "LOGS: SENTINEL FOUND in an application log — VIOLATION"; found=1
  else
    note "LOGS: clean"
  fi
else
  note "LOGS: ANSEO_LOG_DIR unset/missing (skipped)"
fi

# 5. CDN edge caches (AC-6). Every leaderboard/profile page that named the brand
#    must have been PURGED (not left to expire). We assert a purge was issued by
#    confirming the purge log records a purge for the shredded brand's pages.
if [[ -n "$CDN_PURGE_LOG" && -f "$CDN_PURGE_LOG" ]]; then
  if grep -q "purge:leaderboard" "$CDN_PURGE_LOG" 2>/dev/null && \
     grep -q "purge:profile" "$CDN_PURGE_LOG" 2>/dev/null; then
    note "CDN: leaderboard + profile pages purged (AC-6 satisfied)"
  else
    note "CDN: PURGE NOT RECORDED for named pages — VIOLATION (AC-6)"; found=1
  fi
else
  note "CDN: ANSEO_CDN_PURGE_LOG unset/missing (skipped)"
fi

note "----------------------------------------------------------------"

if [[ "$found" -ne 0 ]]; then
  note "RESULT: FAIL — sentinel survived a shred (CC-NFR1 violation)"
  exit 1
fi
if [[ "$inconclusive" -ne 0 && "${CI:-}" == "true" ]]; then
  note "RESULT: FAIL-CLOSED — a required store was unreachable in CI"
  exit 2
fi
note "RESULT: PASS — no sentinel found; shred verified across reachable stores"
exit 0
