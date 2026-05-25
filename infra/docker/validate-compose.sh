#!/usr/bin/env bash
# Phase 1 Story 1.4 — Compose config validation gate.
#
# Runs `docker compose config` against compose.yml and asserts:
#   - the file is syntactically valid
#   - the postgres service image pins to PostgreSQL 16
#   - the redis service image pins to Redis 7.x
#   - every published port binds to 127.0.0.1 by default (localhost-only)
#
# Used both in CI and by the FR-22 release-gate smoke test (Story 4.5).
# Designed for `set -euo pipefail` shells (bash 4+ or modern bash on macOS).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/compose.yml"

if ! command -v docker >/dev/null 2>&1; then
  echo "ERROR: docker is not installed; cannot validate ${COMPOSE_FILE}" >&2
  exit 2
fi

if ! docker compose version >/dev/null 2>&1; then
  echo "ERROR: docker compose plugin is not available" >&2
  exit 2
fi

echo "→ docker compose -f ${COMPOSE_FILE} config"
CONFIG_OUT="$(docker compose -f "${COMPOSE_FILE}" config)"

# AC: PostgreSQL 16 service tag asserted by config review.
if ! echo "${CONFIG_OUT}" | grep -Eq 'image:[[:space:]]+postgres:16(\..*)?(-alpine)?'; then
  echo "FAIL: postgres image is not pinned to a postgres:16(.x)(-alpine) tag" >&2
  echo "${CONFIG_OUT}" | grep -A0 'image:' || true
  exit 1
fi

# AC: Redis 7.x service tag asserted by config review.
if ! echo "${CONFIG_OUT}" | grep -Eq 'image:[[:space:]]+redis:7(\..*)?(-alpine)?'; then
  echo "FAIL: redis image is not pinned to a redis:7(.x)(-alpine) tag" >&2
  echo "${CONFIG_OUT}" | grep -A0 'image:' || true
  exit 1
fi

# AC: localhost-only by default. Every "published" port should be 127.0.0.1.
# `docker compose config` normalizes the bind to a "host_ip" field.
if echo "${CONFIG_OUT}" | grep -E 'host_ip:' | grep -vq '127\.0\.0\.1'; then
  echo "FAIL: at least one published port is not bound to 127.0.0.1" >&2
  echo "${CONFIG_OUT}" | grep -E 'host_ip:' >&2
  exit 1
fi

echo "OK: compose.yml validates; postgres:16, redis:7, all ports bound to 127.0.0.1."
