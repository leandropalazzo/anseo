# Anseo check-visibility GitHub Action
# Docker-based action: builds a minimal image with the `anseo` CLI on
# alpine. Pinning `anseo` to a tagged release here means breaking CLI
# changes don't silently break consumer workflows — bumps go through
# the action's own semver.

FROM alpine:3.20

# `anseo` is fetched from the Anseo GitHub Release assets at build time.
# The release train (.github/workflows/release.yml → cli-binaries job)
# uploads a static musl binary per target triple as a Release asset:
#   anseo-x86_64-unknown-linux-musl
#   anseo-aarch64-unknown-linux-musl
# This MUST be a tag for which that job has run; the very first such tag
# is the next release cut AFTER this change merges (see PR notes). Override
# the version with --build-arg ANSEO_VERSION=<X.Y.Z> in CI.
ARG ANSEO_VERSION=0.6.0

# Map the build arch to the published target triple so the action works on
# both amd64 and arm64 GitHub runners. Fail LOUD with an actionable message
# if the asset is missing (e.g. a tag whose release predates the cli-binaries
# job) instead of shipping a 0-byte binary that 404'd silently.
RUN apk add --no-cache curl ca-certificates jq \
 && arch="$(uname -m)" \
 && case "$arch" in \
      x86_64)  triple="x86_64-unknown-linux-musl" ;; \
      aarch64) triple="aarch64-unknown-linux-musl" ;; \
      *) echo "ERROR: unsupported arch '$arch' — the Anseo action publishes x86_64/aarch64 musl binaries only." >&2; exit 1 ;; \
    esac \
 && url="https://github.com/leandropalazzo/anseo/releases/download/v${ANSEO_VERSION}/anseo-${triple}" \
 && if ! curl -fSL "$url" -o /usr/local/bin/anseo; then \
      echo "ERROR: could not download the Anseo CLI from ${url}" >&2; \
      echo "       The release tag v${ANSEO_VERSION} does not publish an 'anseo-${triple}' asset." >&2; \
      echo "       Standalone CLI binaries are produced by the release train's cli-binaries job;" >&2; \
      echo "       pin ANSEO_VERSION to a tag cut AFTER that job shipped. See infra/github-action/README.md." >&2; \
      exit 1; \
    fi \
 && chmod +x /usr/local/bin/anseo \
 && /usr/local/bin/anseo --version

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
