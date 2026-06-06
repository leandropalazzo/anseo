#!/usr/bin/env bash
# Story 38.16 — standalone production compose validation gate.
#
# Runs `docker compose config` against the standalone compose.yml and asserts
# the production shape:
#   - the file is syntactically valid
#   - there are NO `build:` stanzas (published images only)
#   - every anseo app image resolves to a PINNED version (not :latest, not :dev)
#   - postgres pins to PostgreSQL 16, redis to Redis 7.x
#   - every published port binds to 127.0.0.1 by default (localhost-only)
#
# Resolves variables from .env.example so it runs with no real secrets.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/compose.yml"
ENV_FILE="${SCRIPT_DIR}/.env.example"

if ! command -v docker >/dev/null 2>&1; then
  echo "ERROR: docker is not installed; cannot validate ${COMPOSE_FILE}" >&2
  exit 2
fi

if ! docker compose version >/dev/null 2>&1; then
  echo "ERROR: docker compose plugin is not available" >&2
  exit 2
fi

echo "→ docker compose --env-file ${ENV_FILE} -f ${COMPOSE_FILE} config"
CONFIG_OUT="$(docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" config)"

# AC1: no build: stanzas — published images only.
if echo "${CONFIG_OUT}" | grep -Eq '^[[:space:]]*build:'; then
  echo "FAIL: standalone compose must not contain any build: stanza" >&2
  exit 1
fi

# AC1: every anseo app image must be pinned to a version (not latest/dev).
APP_IMAGES="$(echo "${CONFIG_OUT}" | grep -E 'image:.*/(api|worker|web):' || true)"
if [ -z "${APP_IMAGES}" ]; then
  echo "FAIL: no anseo api/worker/web image references found" >&2
  exit 1
fi
if echo "${APP_IMAGES}" | grep -Eq ':(latest|dev)[[:space:]]*$'; then
  echo "FAIL: an anseo app image is pinned to :latest or :dev" >&2
  echo "${APP_IMAGES}" >&2
  exit 1
fi
if ! echo "${APP_IMAGES}" | grep -Eq ':v[0-9]+\.[0-9]+\.[0-9]+'; then
  echo "FAIL: anseo app images are not pinned to a vX.Y.Z version" >&2
  echo "${APP_IMAGES}" >&2
  exit 1
fi

# Datastore pins.
if ! echo "${CONFIG_OUT}" | grep -Eq 'image:[[:space:]]+postgres:16(\..*)?(-alpine)?'; then
  echo "FAIL: postgres image is not pinned to a postgres:16(.x)(-alpine) tag" >&2
  exit 1
fi
if ! echo "${CONFIG_OUT}" | grep -Eq 'image:[[:space:]]+redis:7(\..*)?(-alpine)?'; then
  echo "FAIL: redis image is not pinned to a redis:7(.x)(-alpine) tag" >&2
  exit 1
fi

# AC6: localhost-only by default. Every published port should be 127.0.0.1.
if echo "${CONFIG_OUT}" | grep -E 'host_ip:' | grep -vq '127\.0\.0\.1'; then
  echo "FAIL: at least one published port is not bound to 127.0.0.1" >&2
  echo "${CONFIG_OUT}" | grep -E 'host_ip:' >&2
  exit 1
fi

echo "OK: standalone compose validates; no build:, app images pinned to vX.Y.Z, postgres:16, redis:7, all ports bound to 127.0.0.1."
