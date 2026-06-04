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
RUN SQLX_OFFLINE=true cargo build --release -p opengeo-worker

## Runtime
FROM debian:bookworm-slim
# libdbus-1-3 (runtime): satisfies the dynamic link from keyring's Linux Secret
# Service backend. ca-certificates: TLS to live provider endpoints + ClickHouse.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libdbus-1-3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/opengeo-worker /usr/local/bin/opengeo-worker
# Shared age-secrets home (compose mounts the api-secrets named volume here so
# the worker reads the same provider keys the dashboard persisted). Owned by the
# unprivileged runtime UID to match the api image's volume ownership.
RUN mkdir -p /data/config && chown -R 1000:1000 /data
USER 1000:1000
ENTRYPOINT ["/usr/local/bin/opengeo-worker"]
