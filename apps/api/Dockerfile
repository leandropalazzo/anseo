## Builder
FROM rust:1.95-bookworm AS builder
# libdbus + pkg-config: needed by the `keyring` crate's Linux Secret Service
# backend (pulled in transitively via opengeo-core). The runtime image doesn't
# need these because the keyring crate is dynamically inert in headless mode.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libdbus-1-dev pkg-config \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .sqlx ./.sqlx
COPY apps ./apps
COPY crates ./crates
# Build the api service binary plus the `ogeo` CLI. The CLI ships in the same
# image so operators can run `docker compose exec api ogeo …` against the
# deployed stack (e.g. `ogeo api key create`) — the api container already
# carries DATABASE_URL, OGEO_CONFIG, and the secrets volume the CLI needs.
RUN SQLX_OFFLINE=true cargo build --release -p opengeo-api -p opengeo-cli

## Runtime
FROM debian:bookworm-slim
# libdbus-1-3 (runtime, not -dev) satisfies the dynamic link from keyring's
# Linux Secret Service backend. wget is used by the compose healthcheck.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libdbus-1-3 wget \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/opengeo-api /usr/local/bin/opengeo-api
# `ogeo` CLI on PATH so `docker compose exec api ogeo …` works against the
# running stack.
COPY --from=builder /workspace/target/release/ogeo /usr/local/bin/ogeo
# Durable home for the age-encrypted provider-secrets file (compose mounts a
# named volume at /data/config via XDG_CONFIG_HOME). Create it owned by the
# unprivileged runtime UID so an empty named volume inherits writable
# ownership on first mount — otherwise the secret-store write fails with
# EACCES and silently falls back to an ephemeral in-memory store.
RUN mkdir -p /data/config && chown -R 1000:1000 /data
EXPOSE 8080
USER 1000:1000
ENV OGEO_BIND_ADDR=0.0.0.0:8080
ENTRYPOINT ["/usr/local/bin/opengeo-api"]
