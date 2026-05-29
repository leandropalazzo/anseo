#!/bin/sh
# OpenGEO check-visibility GitHub Action entrypoint.
#
# Invoked by the runner with the action.yml `args` array:
#   $1 = prompt slug
#   $2 = brand name
#   $3 = expect-rank-lte (integer)
#   $4 = provider filter (empty string = all providers)
#   $5 = api-base URL
#
# Reads OPENGEO_API_KEY from env (consumers wire it via `env:` in the
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
API_BASE="${5:-https://api.opengeo.dev}"

if [ -z "${OPENGEO_API_KEY:-}" ]; then
  echo "::error::OPENGEO_API_KEY env var is required. Set it via the workflow's env: block."
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
RESULT_JSON=$(ogeo "$@" 2>&1) || RC=$?

# Echo the raw CLI output to stderr so CI logs (and the bats harness)
# can see exactly what the action invoked + what it received. Tests pin
# the `--provider <name>` arg against this trace.
printf 'ogeo invoked with: %s\nogeo replied: %s\n' "$*" "$RESULT_JSON" >&2

if [ "$RC" -eq 2 ]; then
  echo "::error::OpenGEO provider error — see CLI output above."
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
    echo "## OpenGEO visibility check"
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
