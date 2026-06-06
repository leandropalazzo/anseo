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
#   - the default project config exists and is mounted read-only into api+worker
#
# Resolves variables from .env.example so it runs with no real secrets.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/compose.yml"
ENV_FILE="${SCRIPT_DIR}/.env.example"
PROJECT_CONFIG="${SCRIPT_DIR}/anseo.example.yaml"

if ! command -v docker >/dev/null 2>&1; then
  echo "ERROR: docker is not installed; cannot validate ${COMPOSE_FILE}" >&2
  exit 2
fi

if ! docker compose version >/dev/null 2>&1; then
  echo "ERROR: docker compose plugin is not available" >&2
  exit 2
fi

# .env.example ships ANSEO_VERSION empty on purpose (no fake default — the stack
# must be pinned to a real published tag). Inject a placeholder so this shape
# gate can still render; it asserts structure, not image existence.
export ANSEO_VERSION="${ANSEO_VERSION:-0.0.0-validate}"

echo "→ docker compose --env-file ${ENV_FILE} -f ${COMPOSE_FILE} config"
CONFIG_OUT="$(docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" config)"

if [ ! -r "${PROJECT_CONFIG}" ]; then
  echo "FAIL: default standalone project config is missing or unreadable: ${PROJECT_CONFIG}" >&2
  exit 1
fi

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
if ! echo "${APP_IMAGES}" | grep -Eq ':[0-9]+\.[0-9]+\.[0-9]+'; then
  echo "FAIL: anseo app images are not pinned to a X.Y.Z version" >&2
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

# AC6b: datastores must STAY localhost even when the operator opens HTTP to the
# world. Render with ANSEO_BIND_HOST=0.0.0.0 (shell env wins over .env) and
# assert that only the api/web ports (8080, 5173) become 0.0.0.0 — postgres
# (5432) and redis (6379) must remain 127.0.0.1. This is the security boundary:
# enabling public access for the proxied HTTP services must never expose the
# unauthenticated datastores off-host.
PUBLIC_OUT="$(ANSEO_BIND_HOST=0.0.0.0 docker compose --env-file "${ENV_FILE}" -f "${COMPOSE_FILE}" config)"
if echo "${PUBLIC_OUT}" | grep -B2 -E 'published: "(5432|6379)"' | grep -q 'host_ip: 0\.0\.0\.0'; then
  echo "FAIL: a datastore port (5432/6379) binds 0.0.0.0 when ANSEO_BIND_HOST=0.0.0.0 — datastores must stay localhost" >&2
  echo "${PUBLIC_OUT}" | grep -E 'host_ip:|published:' >&2
  exit 1
fi
if ! echo "${PUBLIC_OUT}" | grep -B2 -E 'published: "(8080|5173)"' | grep -q 'host_ip: 0\.0\.0\.0'; then
  echo "FAIL: api/web ports did not honor ANSEO_BIND_HOST=0.0.0.0 (proxied deploy would be unreachable)" >&2
  exit 1
fi

CONFIG_MOUNT_COUNT="$(echo "${CONFIG_OUT}" | grep -Ec "source: .*anseo\\.example\\.yaml")"
CONFIG_TARGET_COUNT="$(echo "${CONFIG_OUT}" | grep -Ec 'target: /anseo\.yaml')"
CONFIG_READONLY_COUNT="$(echo "${CONFIG_OUT}" | grep -Ec 'read_only: true')"
if [ "${CONFIG_MOUNT_COUNT}" -lt 2 ] || [ "${CONFIG_TARGET_COUNT}" -lt 2 ] || [ "${CONFIG_READONLY_COUNT}" -lt 2 ]; then
  echo "FAIL: standalone project config must be mounted read-only at /anseo.yaml in api and worker" >&2
  exit 1
fi

for service in api worker; do
  if ! echo "${CONFIG_OUT}" | grep -A80 "^  ${service}:" | grep -Eq 'ANSEO_CONFIG: /anseo\.yaml'; then
    echo "FAIL: ${service} does not read ANSEO_CONFIG=/anseo.yaml" >&2
    exit 1
  fi
done

echo "OK: standalone compose validates; no build:, app images pinned to X.Y.Z, postgres:16, redis:7, all ports bound to 127.0.0.1, project config mounted read-only."
