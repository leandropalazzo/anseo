# Docker Infrastructure

Phase 1 target: local Docker Compose for the OpenGEO stack.

## Services

`compose.yml` brings up five services:

| Service    | Image                | Phase 1 status                                        |
|------------|----------------------|-------------------------------------------------------|
| `postgres` | `postgres:16-alpine` | Real. Healthcheck via `pg_isready`.                   |
| `redis`    | `redis:7-alpine`     | Real. Healthcheck via `redis-cli ping`.               |
| `api`      | `busybox` placeholder | Replaced by `apps/api` image in Story 4.5.            |
| `worker`   | `busybox` placeholder | Replaced by `apps/worker` image when scheduling lands.|
| `web`      | `busybox` placeholder | Replaced by `apps/web` image in Story 4.1 / 4.5.      |

The app services are intentionally placeholders until the corresponding stories build them (Story 1.4 AC: "healthcheck placeholders where app endpoints are not yet implemented"). This keeps stack topology, env wiring, and the FR-22 boot-time test verifiable before app behaviour exists.

## Quick start

```sh
# from this directory
cp .env.example .env       # optional; defaults are fine for a local dev run
docker compose up -d       # boot the stack
docker compose ps          # confirm postgres + redis are healthy
docker compose down        # tear down
```

## Localhost-only by default

All published ports bind to `127.0.0.1` (Story 1.4 AC). Override with the `OGEO_BIND_HOST` env var only after reading the privacy posture in the root `README.md`.

## Environment variables

`compose.yml` reads the following from `.env` (or the shell). All have sensible defaults baked into the compose file, so an empty `.env` boots the stack.

| Variable                | Default      | Purpose                                          |
|-------------------------|--------------|--------------------------------------------------|
| `OGEO_BIND_HOST`        | `127.0.0.1`  | Host interface for every published port.         |
| `POSTGRES_PORT`         | `5432`       | Host port for Postgres.                          |
| `REDIS_PORT`            | `6379`       | Host port for Redis.                             |
| `OGEO_API_PORT`         | `8080`       | Host port for the API.                           |
| `OGEO_WEB_PORT`         | `5173`       | Host port for the web dashboard.                 |
| `POSTGRES_USER`         | `opengeo`    | Postgres role and component of `DATABASE_URL`.   |
| `POSTGRES_PASSWORD`     | `opengeo`    | Postgres password and component of `DATABASE_URL`.|
| `POSTGRES_DB`           | `opengeo`    | Postgres database name.                          |
| `RUST_LOG`              | `info`       | Tracing level for api/worker placeholders.       |
| `OPENGEO_API_IMAGE`     | `busybox:1.36` | Override when a real image exists.             |
| `OPENGEO_WORKER_IMAGE`  | `busybox:1.36` | Override when a real image exists.             |
| `OPENGEO_WEB_IMAGE`     | `busybox:1.36` | Override when a real image exists.             |

The `api` and `worker` services compose their `DATABASE_URL` and `REDIS_URL` from the variables above. The `web` service receives `OGEO_API_BASE_URL=http://api:8080` for in-cluster API calls.

## Migration auto-run path (documented, lands later)

Database migrations live in `crates/storage/migrations/` and are designed to be runnable in two modes:

1. **Programmatic on startup** — once `apps/api` ships its real image (Story 4.5), the API process will call `Storage::migrate()` before binding its HTTP listener. This is the default for the compose stack.
2. **One-shot via the CLI** — `ogeo db migrate` (lands with the CLI in Epic 2) runs the same migrator against `DATABASE_URL`, suitable for CI seed jobs.

A throwaway migration `init` container is **not** added in Phase 1; the API container will own migration on boot. Document this in `apps/api` when the real Dockerfile is authored.

## Validation

`./validate-compose.sh` runs `docker compose config` against `compose.yml` and asserts:

- The compose file is syntactically valid.
- `postgres` is pinned to a `postgres:16(.x)?(-alpine)?` tag.
- `redis` is pinned to a `redis:7(.x)?(-alpine)?` tag.
- Every published port binds to `127.0.0.1` by default.

This is wired into CI and into the FR-22 release-gate smoke test.

## Out of scope (Phase 1)

- Production-grade orchestration (see `infra/k8s/README.md`).
- Cloud-provisioned infrastructure (see `infra/terraform/README.md`).
- TLS termination, multi-host networking, persistent-volume backups beyond named Docker volumes.
- A 60-second boot-time CI gate proving FR-22 on real images — lives in Story 4.5 once the app images exist.
