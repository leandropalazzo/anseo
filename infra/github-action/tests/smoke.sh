#!/bin/sh
# Plain-POSIX-sh smoke harness for the Anseo check-visibility entrypoint.
#
# Exercises the same flows as `tests/action.bats` but without the bats
# dependency, so a contributor without bats installed can still get a
# fast pass/fail before pushing. The bats file is the canonical test
# suite (richer assertions, structured output for CI); this file is the
# fast-feedback companion.
#
# Run from the repo root: sh infra/github-action/tests/smoke.sh

set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENTRYPOINT="$ROOT/entrypoint.sh"

TMP="$(mktemp -d)"
export GITHUB_OUTPUT="$TMP/github_output"
export GITHUB_STEP_SUMMARY="$TMP/github_step_summary"
touch "$GITHUB_OUTPUT" "$GITHUB_STEP_SUMMARY"

STUB_BIN="$TMP/bin"
mkdir -p "$STUB_BIN"
PATH_BEFORE="$PATH"
export PATH="$STUB_BIN:$PATH"

cleanup() {
  export PATH="$PATH_BEFORE"
  rm -rf "$TMP"
}
trap cleanup EXIT

write_stub() {
  cat > "$STUB_BIN/anseo" <<EOF
#!/bin/sh
echo '$1'
exit ${2:-0}
EOF
  chmod +x "$STUB_BIN/anseo"
}

assert_contains() {
  if echo "$1" | grep -q "$2"; then
    return 0
  fi
  echo "ASSERTION FAILED: expected output to contain '$2'"
  echo "Actual: $1"
  return 1
}

PASS=0
FAIL=0

run_case() {
  TITLE="$1"
  shift
  if "$@"; then
    PASS=$((PASS + 1))
    echo "✓ $TITLE"
  else
    FAIL=$((FAIL + 1))
    echo "✗ $TITLE"
  fi
}

# Case: missing ANSEO_API_KEY exits 64
case_missing_key() {
  unset ANSEO_API_KEY
  OUTPUT="$("$ENTRYPOINT" "vec" "Acme" "3" "" "https://api.anseo.ai" 2>&1)" && RC=0 || RC=$?
  [ "$RC" -eq 64 ] && assert_contains "$OUTPUT" "ANSEO_API_KEY"
}

# Case: within threshold exits 0 and writes outputs
case_within_threshold() {
  export ANSEO_API_KEY="anseo_test"
  write_stub '{"observed_rank": 2, "matched_runs": 4}' 0
  OUTPUT="$("$ENTRYPOINT" "vec" "Acme" "3" "" "https://api.anseo.ai" 2>&1)" && RC=0 || RC=$?
  [ "$RC" -eq 0 ] && assert_contains "$(cat "$GITHUB_OUTPUT")" "observed-rank=2"
}

# Case: above threshold exits 1
case_above_threshold() {
  export ANSEO_API_KEY="anseo_test"
  write_stub '{"observed_rank": 7, "matched_runs": 4}' 1
  "$ENTRYPOINT" "vec" "Acme" "3" "" "https://api.anseo.ai" >/dev/null 2>&1 && RC=0 || RC=$?
  [ "$RC" -eq 1 ]
}

# Case: step summary contains the expected table heading
case_summary_table() {
  export ANSEO_API_KEY="anseo_test"
  write_stub '{"observed_rank": 2, "matched_runs": 4}' 0
  "$ENTRYPOINT" "vec" "Acme" "3" "" "https://api.anseo.ai" >/dev/null 2>&1
  assert_contains "$(cat "$GITHUB_STEP_SUMMARY")" "Anseo visibility check"
}

# Case: brand with a space round-trips intact (the bug the bats stub
# couldn't easily catch without the set-- fix).
case_brand_with_space() {
  export ANSEO_API_KEY="anseo_test"
  cat > "$STUB_BIN/anseo" <<'EOF'
#!/bin/sh
echo "{\"observed_rank\": 1, \"matched_runs\": 1, \"saw_brand\": \"$4\"}"
exit 0
EOF
  chmod +x "$STUB_BIN/anseo"
  "$ENTRYPOINT" "vec" "Acme Corp" "3" "" "https://api.anseo.ai" >/dev/null 2>&1
  # If `set --` is wrong, the brand would be word-split and $4 would be
  # something like "--expect-rank-lte" instead of "Acme Corp".
  SUMMARY="$(cat "$GITHUB_STEP_SUMMARY")"
  assert_contains "$SUMMARY" "Acme Corp"
}

run_case "missing ANSEO_API_KEY → exit 64" case_missing_key
run_case "within threshold → exit 0 + output" case_within_threshold
run_case "above threshold → exit 1" case_above_threshold
run_case "step summary rendered" case_summary_table
run_case "brand with space round-trips intact" case_brand_with_space

echo
echo "PASS: $PASS  FAIL: $FAIL"
[ "$FAIL" -eq 0 ]
