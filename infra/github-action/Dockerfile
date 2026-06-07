# OpenGEO check-visibility GitHub Action
# Docker-based action: builds a minimal image with the `ogeo` CLI on
# alpine. Pinning `ogeo` to a tagged release here means breaking CLI
# changes don't silently break consumer workflows — bumps go through
# the action's own semver.

FROM alpine:3.20

# `ogeo` is fetched from the OpenGEO release archive at build time.
# Override via --build-arg OGEO_VERSION=<tag> in CI; the released image
# bakes in a known-good tag.
ARG OGEO_VERSION=0.2.0

RUN apk add --no-cache curl ca-certificates jq \
 && curl -fsSL \
      "https://github.com/opengeo/opengeo/releases/download/v${OGEO_VERSION}/ogeo-x86_64-unknown-linux-musl" \
      -o /usr/local/bin/ogeo \
 && chmod +x /usr/local/bin/ogeo

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
