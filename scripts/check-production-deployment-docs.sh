#!/usr/bin/env bash
# Validate the Story 37.16 production-exposure documentation contract.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOC="${ROOT}/docs/production-deployment.md"
README="${ROOT}/README.md"

require_literal() {
  local file="$1"
  local needle="$2"
  if ! grep -Fq -- "$needle" "$file"; then
    echo "FAIL: ${file#${ROOT}/} missing required text:" >&2
    echo "  ${needle}" >&2
    exit 1
  fi
}

extract_fence() {
  local file="$1"
  local language="$2"
  awk -v fence="\`\`\`${language}" '
    $0 == fence { in_block = 1; next }
    in_block && $0 == "```" { exit }
    in_block { print }
  ' "$file"
}

require_block_literal() {
  local block="$1"
  local label="$2"
  local needle="$3"
  if ! grep -Fq -- "$needle" <<<"$block"; then
    echo "FAIL: ${label} missing required text:" >&2
    echo "  ${needle}" >&2
    exit 1
  fi
}

require_literal "$DOC" "Do **not** expose Anseo to a public network without a reverse proxy, TLS, and auth in front of it."
require_literal "$DOC" "The OSS stack has no built-in authentication for the web dashboard or MCP surfaces; only the \`/v1\` API enforces per-project API keys."
require_literal "$DOC" "## Expose safely — two copy-paste configs"
require_literal "$DOC" '```caddyfile'
require_literal "$DOC" '```nginx'
require_literal "$DOC" "## Minimal API-key gate"
require_literal "$DOC" "The web dashboard and MCP server are **not** key-gated in the OSS stack."
require_literal "$DOC" "## Non-loopback bind warning"
require_literal "$DOC" "WARNING: binding to 0.0.0.0:8080 exposes Anseo on a non-loopback interface."

CADDY_BLOCK="$(extract_fence "$DOC" caddyfile)"
require_block_literal "$CADDY_BLOCK" "caddyfile block" "reverse_proxy localhost:8080"
require_block_literal "$CADDY_BLOCK" "caddyfile block" "@protected {"
require_block_literal "$CADDY_BLOCK" "caddyfile block" "not path /v1* /healthz"
require_block_literal "$CADDY_BLOCK" "caddyfile block" "basicauth @protected {"

NGINX_BLOCK="$(extract_fence "$DOC" nginx)"
require_block_literal "$NGINX_BLOCK" "nginx block" 'auth_basic           "Anseo";'
require_block_literal "$NGINX_BLOCK" "nginx block" 'proxy_pass         http://127.0.0.1:8080;'

for item in \
  "Pinned container images" \
  "Reverse proxy + TLS" \
  "API-key gate enabled" \
  "Secrets injected, not baked" \
  "Postgres backups scheduled"
do
  require_literal "$DOC" "- [ ] **${item}**"
done

require_literal "$README" "## Exposing Anseo safely (security baseline)"
require_literal "$README" "docs/production-deployment.md"

echo "OK: production deployment documentation satisfies the Story 37.16 baseline."
