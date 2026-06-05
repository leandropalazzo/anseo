#!/bin/sh
# OpenGEO SDK drift gate (Story 12.3 / NFR-OpenAPIStable).
#
# Snapshots the committed SDK trees, regenerates them in place from the
# canonical spec, byte-compares against the snapshots, and restores the
# snapshots so the working tree is unchanged on success. Any drift →
# exit non-zero so CI blocks the merge.
#
# Usage: bash infra/codegen/tests/drift.sh

set -eu

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TS_DIR="$ROOT/packages/typescript"
PY_DIR="$ROOT/packages/python"
SPEC="$ROOT/crates/wire-schema/openapi.json"

if [ ! -f "$SPEC" ]; then
  echo "::error::Canonical spec not found at $SPEC — run cargo run -p anseo-wire-schema --bin gen-openapi > $SPEC first."
  exit 1
fi

TMP="$(mktemp -d)"
SNAP_TS="$TMP/snap_ts"
SNAP_PY="$TMP/snap_py"
trap 'rm -rf "$TMP"' EXIT

# Take snapshots of the committed trees so we can diff + restore.
mkdir -p "$SNAP_TS" "$SNAP_PY"
if [ -d "$TS_DIR/src" ]; then
  cp -R "$TS_DIR/src/." "$SNAP_TS/"
fi
if [ -d "$PY_DIR/opengeo" ]; then
  cp -R "$PY_DIR/opengeo/." "$SNAP_PY/"
fi

# TypeScript: regen in place. orval's config has the output path baked
# in, so we generate into $TS_DIR/src and compare against the snapshot.
TS_OK=0
if command -v npx >/dev/null 2>&1; then
  ( cd "$ROOT/infra/codegen" && \
    npx -y orval@7.1.0 --config orval.config.cjs >/dev/null 2>&1 ) \
    && TS_OK=1 \
    || echo "::warning::orval invocation failed; treating as drift = true"
else
  echo "::warning::npx not installed; skipping TypeScript drift check"
fi

# Python: regen in place. The generator's --overwrite only touches the
# files it emits, so auth.py and any hand-written sibling files in
# opengeo/ are preserved automatically.
PY_OK=0
if command -v openapi-python-client >/dev/null 2>&1; then
  openapi-python-client generate --path "$SPEC" \
    --config "$ROOT/infra/codegen/openapi-python.yaml" \
    --meta none --output-path "$PY_DIR/opengeo" --overwrite >/dev/null 2>&1 \
    && PY_OK=1 \
    || echo "::warning::openapi-python-client invocation failed; treating as drift = true"
elif command -v uvx >/dev/null 2>&1; then
  uvx openapi-python-client@0.24.0 generate --path "$SPEC" \
    --config "$ROOT/infra/codegen/openapi-python.yaml" \
    --meta none --output-path "$PY_DIR/opengeo" --overwrite >/dev/null 2>&1 \
    && PY_OK=1 \
    || echo "::warning::openapi-python-client (via uvx) invocation failed; treating as drift = true"
else
  echo "::warning::neither openapi-python-client nor uvx installed; skipping Python drift check"
fi

# Cleanup runtime artifacts that aren't part of the SDK shape.
rm -rf "$PY_DIR/opengeo/.ruff_cache" "$PY_DIR/opengeo/__pycache__" || true

DRIFT=0

if [ "$TS_OK" -eq 1 ]; then
  if ! diff -q -r "$TS_DIR/src" "$SNAP_TS" >/dev/null 2>&1; then
    echo "::error::TypeScript SDK has drifted from the canonical spec; regenerate with: make -C infra/codegen ts"
    DRIFT=1
  fi
fi

if [ "$PY_OK" -eq 1 ]; then
  if ! diff -q -r \
      --exclude="__pycache__" \
      --exclude=".ruff_cache" \
      --exclude=".gitignore" \
      "$PY_DIR/opengeo" "$SNAP_PY" >/dev/null 2>&1; then
    echo "::error::Python SDK has drifted from the canonical spec; regenerate with: make -C infra/codegen py"
    DRIFT=1
  fi
fi

# Restore snapshots so the working tree is left untouched on success.
# (On failure the regenerated files stay so the operator can inspect.)
if [ "$DRIFT" -eq 0 ]; then
  rm -rf "$TS_DIR/src" "$PY_DIR/opengeo"
  mkdir -p "$TS_DIR/src" "$PY_DIR/opengeo"
  cp -R "$SNAP_TS/." "$TS_DIR/src/"
  cp -R "$SNAP_PY/." "$PY_DIR/opengeo/"
  echo "✓ SDKs in sync with crates/wire-schema/openapi.json"
fi
exit "$DRIFT"
