#!/usr/bin/env bash
# Verify published Story 38.15 GHCR images after a release tag has run.
#
# Usage:
#   scripts/verify-release-images.sh 0.6.0
#   ANSEO_IMAGE_REGISTRY=ghcr.io/leandropalazzo/anseo scripts/verify-release-images.sh 0.6.0

set -euo pipefail

VERSION="${1:-${ANSEO_VERSION:-}}"
REGISTRY="${ANSEO_IMAGE_REGISTRY:-ghcr.io/leandropalazzo/anseo}"
APPS=(api worker web)
FORBIDDEN_PATTERNS=(
  "OPENGEO_KEYRING_PASSPHRASE"
  "ANSEO_KEYRING_PASSPHRASE"
  "dev-compose-secrets-passphrase"
  ":dev"
)

if [[ -z "$VERSION" ]]; then
  echo "usage: $0 <X.Y.Z>" >&2
  exit 64
fi

if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "FAIL: version must be a bare X.Y.Z tag, got '$VERSION'" >&2
  exit 64
fi

for tool in docker jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "FAIL: missing required tool: $tool" >&2
    exit 69
  fi
done

docker_config="${DOCKER_CONFIG:-$HOME/.docker}/config.json"
if [[ "${ALLOW_GHCR_AUTH:-0}" != "1" && -f "$docker_config" ]] && grep -Fq "ghcr.io" "$docker_config"; then
  echo "FAIL: Docker config contains ghcr.io credentials." >&2
  echo "Run this verifier from a clean Docker config, or set ALLOW_GHCR_AUTH=1 when intentionally checking with auth." >&2
  exit 78
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

for app in "${APPS[@]}"; do
  image="${REGISTRY}/${app}:${VERSION}"
  manifest="${tmpdir}/${app}.manifest.json"

  echo "== ${image}"
  docker manifest inspect "$image" > "$manifest"

  for platform in "linux/amd64" "linux/arm64"; do
    os="${platform%/*}"
    arch="${platform#*/}"
    if ! jq -e --arg os "$os" --arg arch "$arch" \
      '.manifests[]? | select(.platform.os == $os and .platform.architecture == $arch)' \
      "$manifest" >/dev/null; then
      echo "FAIL: ${image} missing ${platform} manifest" >&2
      exit 1
    fi
  done

  docker pull --platform linux/amd64 "$image" >/dev/null
  docker history --no-trunc "$image" > "${tmpdir}/${app}.history.txt"
  for forbidden in "${FORBIDDEN_PATTERNS[@]}"; do
    if grep -Fq "$forbidden" "${tmpdir}/${app}.history.txt"; then
      echo "FAIL: ${image} history contains forbidden text: ${forbidden}" >&2
      exit 1
    fi
  done
done

echo "OK: ${REGISTRY}/{api,worker,web}:${VERSION} are pullable, multi-arch, and pass history checks."
