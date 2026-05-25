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
RUN SQLX_OFFLINE=true cargo build --release -p opengeo-api

## Runtime
FROM debian:bookworm-slim
# libdbus-1-3 (runtime, not -dev) satisfies the dynamic link from keyring's
# Linux Secret Service backend. wget is used by the compose healthcheck.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libdbus-1-3 wget \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/opengeo-api /usr/local/bin/opengeo-api
EXPOSE 8080
USER 1000:1000
ENV OGEO_BIND_ADDR=0.0.0.0:8080
ENTRYPOINT ["/usr/local/bin/opengeo-api"]
