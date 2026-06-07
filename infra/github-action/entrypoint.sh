#!/bin/sh
# OpenGEO check-visibility GitHub Action entrypoint.
#
# Invoked by the runner with the action.yml `args` array:
#   $1 = prompt slug
#   $2 = brand name
#   $3 = expect-rank-lte (integer)
#   $4 = provider filter (empty string = all providers)
#   $5 = api-base URL
#   $6 = audit URL (mode=audit)
#   $7 = audit fail-on rule/severity (mode=audit)
#   $8 = audit max-pages (mode=audit)
#   $9 = mode (visibility|audit)
#
# Reads ANSEO_API_KEY from env (consumers wire it via `env:` in the
# workflow step). Writes a $GITHUB_STEP_SUMMARY entry per FR-44 so the
# PR view shows the result inline.
#
# POSIX sh only — no bash-isms. Runs on the alpine:3.20 base image
# which only ships /bin/sh (busybox ash). Failure modes:
#   - exit 1: visibility check failed (rank worse than threshold).
#     Standard CI failure.
#   - exit 2: provider returned an error (auth, rate-limit, 5xx). Lets
#     consumers distinguish "config problem" from "you regressed".
#   - exit 64: missing required input (API key, malformed args).

set -eu

PROMPT="${1:-}"
BRAND="${2:-}"
EXPECT_RANK_LTE="${3:-}"
PROVIDER="${4:-}"
API_BASE="${5:-https://api.anseo.ai}"
AUDIT_URL="${6:-}"
AUDIT_FAIL_ON="${7:-high}"
AUDIT_MAX_PAGES="${8:-25}"
MODE="${9:-visibility}"

if [ "$MODE" = "audit" ]; then
  if [ -z "$AUDIT_URL" ]; then
    echo "::error::audit-url is required when mode=audit."
    exit 64
  fi

  set -- audit "$AUDIT_URL" \
      --fail-on "$AUDIT_FAIL_ON" \
      --max-pages "$AUDIT_MAX_PAGES" \
      --format json

  RC=0
  ERR_FILE="$(mktemp)"
  RESULT_JSON=$(anseo "$@" 2>"$ERR_FILE") || RC=$?
  ERROR_TEXT=$(cat "$ERR_FILE")
  rm -f "$ERR_FILE"
  printf 'anseo invoked with: %s\nanseo replied: %s\n%s\n' "$*" "$RESULT_JSON" "$ERROR_TEXT" >&2

  AUDIT_SCORE=$(echo "$RESULT_JSON" | jq -r '.overall_score // 0' 2>/dev/null || echo 0)
  AUDIT_FAILED_FINDINGS=$(echo "$RESULT_JSON" | jq -r '.gate.failed_findings | length // 0' 2>/dev/null || echo 0)

  {
    echo "audit-score=$AUDIT_SCORE"
    echo "audit-failed-findings=$AUDIT_FAILED_FINDINGS"
  } >> "${GITHUB_OUTPUT:-/dev/null}"

  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
      echo "## Anseo site audit"
      echo ""
      echo "| Field | Value |"
      echo "| ----- | ----- |"
      echo "| URL | \`$AUDIT_URL\` |"
      echo "| Fail on | \`$AUDIT_FAIL_ON\` |"
      echo "| Max pages | $AUDIT_MAX_PAGES |"
      echo "| Overall score | $AUDIT_SCORE |"
      echo "| Failed findings | $AUDIT_FAILED_FINDINGS |"
      echo ""
      if [ "$RC" -eq 0 ]; then
        echo "✓ Audit gate passed."
      else
        echo "✗ Audit gate failed. CI build failed."
      fi
    } >> "$GITHUB_STEP_SUMMARY"
  fi

  exit "$RC"
fi

if [ "$MODE" != "visibility" ]; then
  echo "::error::mode must be either visibility or audit."
  exit 64
fi

if [ -z "${ANSEO_API_KEY:-}" ]; then
  echo "::error::ANSEO_API_KEY env var is required. Set it via the workflow's env: block."
  exit 64
fi
if [ -z "$PROMPT" ] || [ -z "$BRAND" ] || [ -z "$EXPECT_RANK_LTE" ]; then
  echo "::error::Missing required input. Need prompt, brand, expect-rank-lte."
  exit 64
fi

# Build argv with `set --` so brand names with spaces ("Acme Corp")
# and any future quoted arg pass through intact. The CLI receives one
# proper argv element per flag value — no word-splitting hazards.
set -- check visibility \
    --prompt "$PROMPT" \
    --brand "$BRAND" \
    --expect-rank-lte "$EXPECT_RANK_LTE" \
    --api-base "$API_BASE" \
    --json
if [ -n "$PROVIDER" ]; then
  set -- "$@" --provider "$PROVIDER"
fi

# Run the CLI; capture stdout (JSON) for parsing + propagate stderr.
RC=0
RESULT_JSON=$(anseo "$@" 2>&1) || RC=$?

# Echo the raw CLI output to stderr so CI logs (and the bats harness)
# can see exactly what the action invoked + what it received. Tests pin
# the `--provider <name>` arg against this trace.
printf 'anseo invoked with: %s\nanseo replied: %s\n' "$*" "$RESULT_JSON" >&2

if [ "$RC" -eq 2 ]; then
  echo "::error::Anseo provider error — see CLI output above."
  exit 2
fi

OBSERVED_RANK=$(echo "$RESULT_JSON" | jq -r '.observed_rank // "null"')
MATCHED_RUNS=$(echo "$RESULT_JSON" | jq -r '.matched_runs // 0')

# Surface the result to the next step via outputs.
{
  echo "observed-rank=$OBSERVED_RANK"
  echo "matched-runs=$MATCHED_RUNS"
} >> "${GITHUB_OUTPUT:-/dev/null}"

# Inline-render the result in the PR step summary (FR-44).
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "## Anseo visibility check"
    echo ""
    echo "| Field | Value |"
    echo "| ----- | ----- |"
    echo "| Prompt | \`$PROMPT\` |"
    echo "| Brand | \`$BRAND\` |"
    echo "| Provider | \`${PROVIDER:-all}\` |"
    echo "| Threshold | rank ≤ $EXPECT_RANK_LTE |"
    echo "| Observed rank | $OBSERVED_RANK |"
    echo "| Matched runs | $MATCHED_RUNS |"
    echo ""
    if [ "$RC" -eq 0 ]; then
      echo "✓ Brand ranking within threshold."
    else
      echo "✗ Brand ranking below threshold. CI build failed."
    fi
  } >> "$GITHUB_STEP_SUMMARY"
fi

exit "$RC"
