#!/usr/bin/env bash
# Wait for the docker-compose stack to report all services healthy within $1 seconds.
# Used by .github/workflows/test.yml to enforce architecture.md Phase 1 acceptance:
# "Docker Compose boots within 60 s on a 2 CPU / 4 GB host."

set -euo pipefail

deadline_seconds="${1:-60}"
# Locate the compose file. Honor an override but default to the layout the
# rest of the repo assumes.
COMPOSE_FILE="${COMPOSE_FILE:-infra/docker/compose.yml}"
if [[ ! -f "${COMPOSE_FILE}" ]]; then
  echo "::error::compose file not found at ${COMPOSE_FILE} (set COMPOSE_FILE to override)"
  exit 2
fi

start=$(date +%s)

while true; do
  now=$(date +%s)
  elapsed=$((now - start))
  if (( elapsed > deadline_seconds )); then
    echo "::error::stack did not become healthy within ${deadline_seconds}s"
    docker compose -f "${COMPOSE_FILE}" ps
    exit 1
  fi

  unhealthy=$(docker compose -f "${COMPOSE_FILE}" ps --format json 2>/dev/null \
    | jq -r 'select(.Health and .Health != "healthy") | .Service' \
    | wc -l \
    | tr -d ' ')

  if [[ "${unhealthy}" == "0" ]]; then
    echo "stack healthy in ${elapsed}s (budget: ${deadline_seconds}s)"
    exit 0
  fi

  sleep 1
done
