# Anseo check-visibility GitHub Action
# Docker-based action: builds a minimal image with the `anseo` CLI on
# alpine. Pinning `anseo` to a tagged release here means breaking CLI
# changes don't silently break consumer workflows — bumps go through
# the action's own semver.

FROM alpine:3.20

# `anseo` is fetched from the Anseo release archive at build time.
# Override via --build-arg ANSEO_VERSION=<tag> in CI; the released image
# bakes in a known-good tag.
ARG ANSEO_VERSION=0.2.0

RUN apk add --no-cache curl ca-certificates jq \
 && curl -fsSL \
      "https://github.com/leandropalazzo/anseo/releases/download/v${ANSEO_VERSION}/anseo-x86_64-unknown-linux-musl" \
      -o /usr/local/bin/anseo \
 && chmod +x /usr/local/bin/anseo

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
