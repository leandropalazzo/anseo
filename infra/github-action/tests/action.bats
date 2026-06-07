#!/usr/bin/env bats
# Unit tests for the Anseo check-visibility action's entrypoint.sh.
#
# Run with: bats infra/github-action/tests/action.bats
#
# The entrypoint shells out to `anseo`; we stub the binary on PATH so the
# tests run without the real CLI. Each test sets up a temp dir for
# GITHUB_OUTPUT + GITHUB_STEP_SUMMARY and inspects the resulting files.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/.." && pwd)"
  ENTRYPOINT="$ROOT/entrypoint.sh"

  TMP_DIR="$(mktemp -d)"
  export GITHUB_OUTPUT="$TMP_DIR/github_output"
  export GITHUB_STEP_SUMMARY="$TMP_DIR/github_step_summary"
  touch "$GITHUB_OUTPUT" "$GITHUB_STEP_SUMMARY"

  # Stub the `anseo` binary by prepending a tmp bin/ to PATH. Each test
  # writes its own stub to control the JSON output + exit code.
  STUB_BIN="$TMP_DIR/bin"
  mkdir -p "$STUB_BIN"
  PATH_BEFORE="$PATH"
  export PATH="$STUB_BIN:$PATH"
  export ANSEO_API_KEY="anseo_test_key"
}

teardown() {
  export PATH="$PATH_BEFORE"
  rm -rf "$TMP_DIR"
}

write_anseo_stub() {
  local json="$1"
  local exit_code="${2:-0}"
  cat > "$STUB_BIN/anseo" <<EOF
#!/bin/sh
echo '$json'
exit $exit_code
EOF
  chmod +x "$STUB_BIN/anseo"
}

@test "missing ANSEO_API_KEY exits 64" {
  unset ANSEO_API_KEY
  run "$ENTRYPOINT" "vec-db" "Pinecone" "3" "" "https://api.anseo.ai"
  [ "$status" -eq 64 ]
  [[ "$output" == *"ANSEO_API_KEY"* ]]
}

@test "missing prompt arg exits 64" {
  run "$ENTRYPOINT" "" "Pinecone" "3" "" "https://api.anseo.ai"
  [ "$status" -eq 64 ]
  [[ "$output" == *"Missing required input"* ]]
}

@test "within-threshold result exits 0 and writes outputs" {
  write_anseo_stub '{"observed_rank": 2, "matched_runs": 4}' 0

  run "$ENTRYPOINT" "vec-db" "Pinecone" "3" "" "https://api.anseo.ai"
  [ "$status" -eq 0 ]

  run cat "$GITHUB_OUTPUT"
  [[ "$output" == *"observed-rank=2"* ]]
  [[ "$output" == *"matched-runs=4"* ]]
}

@test "above-threshold result exits 1" {
  write_anseo_stub '{"observed_rank": 7, "matched_runs": 4}' 1

  run "$ENTRYPOINT" "vec-db" "Pinecone" "3" "" "https://api.anseo.ai"
  [ "$status" -eq 1 ]
}

@test "provider error exits 2 with operator-distinguishable message" {
  write_anseo_stub '{"error":"provider_unauthorized"}' 2

  run "$ENTRYPOINT" "vec-db" "Pinecone" "3" "openai" "https://api.anseo.ai"
  [ "$status" -eq 2 ]
  [[ "$output" == *"provider error"* ]]
}

@test "step summary contains rendered table" {
  write_anseo_stub '{"observed_rank": 2, "matched_runs": 4}' 0

  run "$ENTRYPOINT" "vec-db" "Pinecone" "3" "" "https://api.anseo.ai"
  [ "$status" -eq 0 ]

  run cat "$GITHUB_STEP_SUMMARY"
  [[ "$output" == *"Anseo visibility check"* ]]
  [[ "$output" == *"vec-db"* ]]
  [[ "$output" == *"Pinecone"* ]]
  [[ "$output" == *"Observed rank"* ]]
}

@test "step summary marks below-threshold as failed" {
  write_anseo_stub '{"observed_rank": 7, "matched_runs": 4}' 1

  run "$ENTRYPOINT" "vec-db" "Pinecone" "3" "" "https://api.anseo.ai"
  [ "$status" -eq 1 ]

  run cat "$GITHUB_STEP_SUMMARY"
  [[ "$output" == *"below threshold"* ]]
}

@test "absent observed_rank renders as 'null'" {
  write_anseo_stub '{"observed_rank": null, "matched_runs": 0}' 0

  run "$ENTRYPOINT" "vec-db" "Pinecone" "3" "" "https://api.anseo.ai"
  [ "$status" -eq 0 ]

  run cat "$GITHUB_OUTPUT"
  [[ "$output" == *"observed-rank=null"* ]]
}

@test "visibility invokes the CLI with only its supported flags" {
  cat > "$STUB_BIN/anseo" <<'EOF'
#!/bin/sh
# Echo the args we were called with so the test can inspect them.
echo "{\"observed_rank\": 1, \"matched_runs\": 1, \"called_with\": \"$*\"}"
exit 0
EOF
  chmod +x "$STUB_BIN/anseo"

  run "$ENTRYPOINT" "vec-db" "Pinecone" "3" "anthropic" "https://api.anseo.ai"
  [ "$status" -eq 0 ]
  [[ "$output" == *"check visibility"* ]]
  [[ "$output" == *"--prompt vec-db"* ]]
  [[ "$output" == *"--brand Pinecone"* ]]
  [[ "$output" == *"--expect-rank-lte 3"* ]]
  # The CLI's check-visibility stub does not accept these yet (ships in Story 3.2),
  # so the entrypoint must NOT forward them or clap rejects the invocation.
  [[ "$output" != *"--provider"* ]]
  [[ "$output" != *"--api-base"* ]]
  [[ "$output" != *"--json"* ]]
}

@test "audit mode does not require API key and forwards gate flags" {
  unset ANSEO_API_KEY
  cat > "$STUB_BIN/anseo" <<'EOF'
#!/bin/sh
echo '{"overall_score": 86, "gate": {"passed": true, "failed_findings": []}}'
exit 0
EOF
  chmod +x "$STUB_BIN/anseo"

  run "$ENTRYPOINT" "" "" "" "" "https://api.anseo.ai" "https://example.com/sitemap.xml" "medium" "7" "audit"
  [ "$status" -eq 0 ]
  [[ "$output" == *"audit https://example.com/sitemap.xml --fail-on medium --max-pages 7 --format json"* ]]

  run cat "$GITHUB_OUTPUT"
  [[ "$output" == *"audit-score=86"* ]]
  [[ "$output" == *"audit-failed-findings=0"* ]]
}

@test "audit mode parses JSON outputs when gate fails" {
  unset ANSEO_API_KEY
  cat > "$STUB_BIN/anseo" <<'EOF'
#!/bin/sh
echo '{"overall_score": 42, "gate": {"passed": false, "failed_findings": [{"rule_id": "corroboration.outbound_links"}]}}'
echo 'error: visibility check failed: audit gate failed for 1 finding(s)' >&2
exit 1
EOF
  chmod +x "$STUB_BIN/anseo"

  run "$ENTRYPOINT" "" "" "" "" "https://api.anseo.ai" "https://example.com" "high" "5" "audit"
  [ "$status" -eq 1 ]

  run cat "$GITHUB_OUTPUT"
  [[ "$output" == *"audit-score=42"* ]]
  [[ "$output" == *"audit-failed-findings=1"* ]]
}
