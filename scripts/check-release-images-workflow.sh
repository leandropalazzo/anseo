#!/usr/bin/env bash
# Validate the Story 38.15 release-image workflow contract.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RELEASE="${ROOT}/.github/workflows/release.yml"
IMAGES="${ROOT}/.github/workflows/release-images.yml"
DOCKERIGNORE="${ROOT}/.dockerignore"

require_literal() {
  local file="$1"
  local needle="$2"
  if ! grep -Fq -- "$needle" "$file"; then
    echo "FAIL: ${file#"${ROOT}"/} missing required text:" >&2
    echo "  ${needle}" >&2
    exit 1
  fi
}

require_line() {
  local file="$1"
  local needle="$2"
  if ! grep -Fxq -- "$needle" "$file"; then
    echo "FAIL: ${file#"${ROOT}"/} missing required line:" >&2
    echo "  ${needle}" >&2
    exit 1
  fi
}

require_absent() {
  local file="$1"
  local needle="$2"
  if grep -Fq -- "$needle" "$file"; then
    echo "FAIL: ${file#"${ROOT}"/} must not contain:" >&2
    echo "  ${needle}" >&2
    exit 1
  fi
}

require_no_regex() {
  local file="$1"
  local pattern="$2"
  if grep -Eq -- "$pattern" "$file"; then
    echo "FAIL: ${file#"${ROOT}"/} must not match pattern:" >&2
    echo "  ${pattern}" >&2
    exit 1
  fi
}

require_regex() {
  local file="$1"
  local pattern="$2"
  if ! grep -Eq -- "$pattern" "$file"; then
    echo "FAIL: ${file#"${ROOT}"/} missing required pattern:" >&2
    echo "  ${pattern}" >&2
    exit 1
  fi
}

for file in "$RELEASE" "$IMAGES" "$DOCKERIGNORE"; do
  if [[ ! -f "$file" ]]; then
    echo "FAIL: missing ${file#"${ROOT}"/}" >&2
    exit 1
  fi
done

RUBY_BIN="${RUBY_BIN:-ruby}"

if ! command -v "$RUBY_BIN" >/dev/null 2>&1; then
  echo "FAIL: ruby is required to structurally validate GitHub workflow YAML" >&2
  exit 1
fi

"$RUBY_BIN" - "$RELEASE" "$IMAGES" <<'RUBY'
require "yaml"

release_path, images_path = ARGV

def load_workflow(path)
  YAML.load_file(path)
end

def workflow_on(doc)
  doc["on"] || doc[true]
end

def assert(condition, message)
  unless condition
    warn "FAIL: #{message}"
    exit 1
  end
end

release = load_workflow(release_path)
images = load_workflow(images_path)

release_on = workflow_on(release)
assert(release_on.is_a?(Hash), "release.yml must declare workflow triggers")
assert(release_on.dig("push", "tags") == ["v*"], "release.yml must publish from v* tag pushes")
assert(release_on.key?("workflow_dispatch"), "release.yml must support guarded workflow_dispatch")

release_permissions = release.fetch("permissions")
assert(release_permissions["packages"] == "write", "release.yml must grant packages: write")
assert(release_permissions["attestations"] == "write", "release.yml must grant attestations: write")
assert(release_permissions["id-token"] == "write", "release.yml must grant id-token: write")

images_job = release.dig("jobs", "images")
assert(images_job, "release.yml must define the images job")
assert(images_job["uses"] == "./.github/workflows/release-images.yml", "images job must call release-images.yml")
assert(
  images_job.dig("with", "push") == "${{ needs.resolve.outputs.dry_run != 'true' && needs.resolve.outputs.publish_ok == 'true' }}",
  "images job push expression must be guarded by dry_run and publish_ok"
)
assert(
  images_job.dig("with", "tag_latest") == "${{ needs.resolve.outputs.tag_latest == 'true' && needs.resolve.outputs.dry_run != 'true' && needs.resolve.outputs.publish_ok == 'true' }}",
  "images job latest expression must be guarded by tag_latest, dry_run, and publish_ok"
)

images_on = workflow_on(images)
assert(images_on.is_a?(Hash), "release-images.yml must declare workflow_call")
assert(images_on.keys == ["workflow_call"], "release-images.yml must be workflow_call-only")

images_permissions = images.fetch("permissions")
assert(images_permissions["packages"] == "write", "release-images.yml must grant packages: write")
assert(images_permissions["attestations"] == "write", "release-images.yml must grant attestations: write")
assert(images_permissions["id-token"] == "write", "release-images.yml must grant id-token: write")

build_job = images.dig("jobs", "build-and-push")
assert(build_job, "release-images.yml must define build-and-push")
assert(build_job.dig("strategy", "matrix", "app") == ["api", "worker", "web"], "image matrix must be api/worker/web")

steps = build_job.fetch("steps")
login = steps.find { |step| step["uses"].to_s.start_with?("docker/login-action@") }
assert(login, "release-images.yml must include GHCR login")
assert(login["if"] == "${{ inputs.push }}", "GHCR login must run only when publishing")
assert(login.dig("with", "registry") == "ghcr.io", "GHCR login must target ghcr.io")
assert(login.dig("with", "username") == "${{ github.actor }}", "GHCR login must use github.actor")
assert(login.dig("with", "password") == "${{ secrets.GITHUB_TOKEN }}", "GHCR login must use GITHUB_TOKEN")

metadata = steps.find { |step| step["id"] == "meta" }
assert(metadata, "release-images.yml must include Docker metadata step")
tag_lines = metadata.dig("with", "tags").to_s.lines.map(&:strip)
assert(tag_lines.include?("type=semver,pattern={{version}},value=v${{ steps.version.outputs.version }}"), "metadata tags must include X.Y.Z")
assert(tag_lines.include?("type=semver,pattern={{major}}.{{minor}},value=v${{ steps.version.outputs.version }}"), "metadata tags must include X.Y")
assert(tag_lines.include?("type=semver,pattern={{major}},value=v${{ steps.version.outputs.version }}"), "metadata tags must include X")
assert(tag_lines.include?("type=raw,value=latest,enable=${{ inputs.tag_latest }}"), "metadata tags must gate latest on inputs.tag_latest")

build = steps.find { |step| step["id"] == "push" }
assert(build, "release-images.yml must include build-push step")
assert(build.dig("with", "platforms") == "linux/amd64,linux/arm64", "build-push must target linux/amd64 and linux/arm64")
assert(build.dig("with", "push") == "${{ inputs.push == true }}", "build-push must respect inputs.push")

attest = steps.find { |step| step["uses"].to_s.start_with?("actions/attest-build-provenance@") }
assert(attest, "release-images.yml must attest build provenance")
assert(attest["if"] == "${{ inputs.push }}", "attestation must run only when publishing")
assert(attest.dig("with", "push-to-registry") == true, "attestation must push provenance to registry")
RUBY

# The release train is the only tag entry point.
require_literal "$RELEASE" "name: release"
require_literal "$RELEASE" "push:"
require_literal "$RELEASE" '      - "v*"'
require_literal "$RELEASE" "workflow_dispatch:"
require_literal "$RELEASE" "packages: write # images push to GHCR"
require_literal "$RELEASE" "attestations: write"
require_literal "$RELEASE" "id-token: write"
require_literal "$RELEASE" "publish_ok:"
require_literal "$RELEASE" "tag_latest:"
require_literal "$RELEASE" "uses: ./.github/workflows/release-images.yml"
require_literal "$RELEASE" "push: \${{ needs.resolve.outputs.dry_run != 'true' && needs.resolve.outputs.publish_ok == 'true' }}"
require_literal "$RELEASE" "tag_latest: \${{ needs.resolve.outputs.tag_latest == 'true' && needs.resolve.outputs.dry_run != 'true' && needs.resolve.outputs.publish_ok == 'true' }}"

# The image workflow is reusable only, so tag pushes cannot double-publish.
require_literal "$IMAGES" "name: release-images"
require_literal "$IMAGES" "workflow_call:"
require_no_regex "$IMAGES" "^  workflow_dispatch:"
require_no_regex "$IMAGES" "^  pull_request:"
require_no_regex "$IMAGES" "^  push:"

# It must publish the three app images to GHCR with no external registry secret.
require_literal "$IMAGES" "app: [api, worker, web]"
require_literal "$IMAGES" "registry: ghcr.io"
require_literal "$IMAGES" "username: \${{ github.actor }}"
require_literal "$IMAGES" "password: \${{ secrets.GITHUB_TOKEN }}"
require_absent "$IMAGES" "DOCKERHUB"
require_absent "$IMAGES" "REGISTRY_TOKEN"
require_literal "$IMAGES" "images: ghcr.io/\${{ github.repository_owner }}/anseo/\${{ matrix.app }}"

# Tags: X.Y.Z, X.Y, X, and latest only when the caller explicitly allows it.
require_literal "$IMAGES" "type=semver,pattern={{version}},value=v\${{ steps.version.outputs.version }}"
require_literal "$IMAGES" "type=semver,pattern={{major}}.{{minor}},value=v\${{ steps.version.outputs.version }}"
require_literal "$IMAGES" "type=semver,pattern={{major}},value=v\${{ steps.version.outputs.version }}"
require_literal "$IMAGES" "type=raw,value=latest,enable=\${{ inputs.tag_latest }}"

# Multi-arch build, dry-run support, and provenance attestation.
require_literal "$IMAGES" "uses: docker/setup-qemu-action@v3"
require_literal "$IMAGES" "uses: docker/setup-buildx-action@v3"
require_literal "$IMAGES" "platforms: linux/amd64,linux/arm64"
require_literal "$IMAGES" "push: \${{ inputs.push == true }}"
require_regex "$IMAGES" "if:[[:space:]]*\\$\\{\\{ inputs\\.push \\}\\}"
require_literal "$IMAGES" "uses: actions/attest-build-provenance@v1"
require_literal "$IMAGES" "push-to-registry: true"

# Build context hardening: no VCS metadata, Actions config, or local env files.
require_literal "$IMAGES" "persist-credentials: false"
require_line "$DOCKERIGNORE" ".git"
require_line "$DOCKERIGNORE" ".github"
require_line "$DOCKERIGNORE" ".env"
require_line "$DOCKERIGNORE" "**/.env"
require_line "$DOCKERIGNORE" "*.pem"
require_line "$DOCKERIGNORE" "*.key"

# Image Dockerfiles must not bake legacy secrets or development image tags.
for dockerfile in \
  "$ROOT/apps/api/Dockerfile" \
  "$ROOT/apps/worker/Dockerfile" \
  "$ROOT/apps/web/Dockerfile"
do
  require_absent "$dockerfile" "OPENGEO_KEYRING_PASSPHRASE"
  require_absent "$dockerfile" "ANSEO_KEYRING_PASSPHRASE"
  require_absent "$dockerfile" ":dev"
done

echo "OK: release image workflow satisfies the Story 38.15 GHCR contract."
