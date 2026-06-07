# Docker Infrastructure (Tier 2, dev)

Local Docker Compose for the full Anseo stack, built from source. For a
production, no-clone install with published images, see
[`infra/standalone/`](../standalone/) and [`docs/manual/deploy.md`](../../docs/manual/deploy.md).

## Services

`compose.yml` brings up five services:

| Service    | Image (default)      | Role                                                          |
|------------|----------------------|--------------------------------------------------------------|
| `postgres` | `postgres:16-alpine` | Primary datastore. Healthcheck via `pg_isready`.             |
| `redis`    | `redis:7-alpine`     | Queue/cache. Healthcheck via `redis-cli ping`.              |
| `api`      | `anseo/api:dev`      | Axum REST `/v1` API. Runs DB migrations on boot. Port 8080. |
| `worker`   | `anseo/worker:dev`   | Background worker (scheduled runs, alert/webhook delivery).  |
| `web`      | `anseo/web:dev`      | Next.js dashboard. Port 5173.                               |

The `api`, `worker`, and `web` services build from source by default (their
`build:` stanzas point at `apps/{api,worker,web}`); override `ANSEO_*_IMAGE` to
pull a published image instead. The `api` container owns schema migration — it
calls the migrator before binding its HTTP listener, so a fresh volume is
migrated automatically on first boot.

## Quick start

```sh
# from this directory
cp .env.example .env       # optional; defaults are fine for a local dev run
docker compose up -d       # boot the stack
docker compose ps          # confirm postgres + redis are healthy
docker compose down        # tear down
```

> **Upgrading from a pre-rename stack.** The Postgres role/database default
> changed from `opengeo`/`opengeo_test` to `anseo`/`anseo_test`. Postgres only
> applies `POSTGRES_USER`/`POSTGRES_DB` when the data volume is first
> initialized, so an existing `postgres-data` volume created before this change
> still holds the old `opengeo` role and the api/worker will fail to connect.
> Local dev data is disposable — recreate the volume to pick up the new
> defaults: `docker compose down -v && docker compose up -d` (or rename the role
> + db inside the existing volume if you need to keep the data).

For day-to-day local deployment from the workspace root, prefer the deploy
helper:

```sh
scripts/local-deploy.sh
scripts/local-deploy.sh --plan
scripts/local-deploy.sh --force-build
```

The helper checks that Docker is reachable, starts OrbStack on macOS when it is
installed but not running, fingerprints the local inputs for `api`, `worker`,
and `web`, rebuilds only stale app images, then brings the Compose stack up and
waits for service health checks. Its fingerprint state lives under
`.git/opengeo-local-deploy`, so it does not add working-tree files.

## Localhost-only by default

All published ports bind to `127.0.0.1` (Story 1.4 AC). Override with the `ANSEO_BIND_HOST` env var only after reading the privacy posture in the root `README.md`.

## Environment variables

`compose.yml` reads the following from `.env` (or the shell). All have sensible defaults baked into the compose file, so an empty `.env` boots the stack.

| Variable                | Default          | Purpose                                          |
|-------------------------|------------------|--------------------------------------------------|
| `ANSEO_BIND_HOST`       | `127.0.0.1`      | Host interface for every published port.         |
| `POSTGRES_PORT`         | `5432`           | Host port for Postgres.                          |
| `REDIS_PORT`            | `6379`           | Host port for Redis.                             |
| `ANSEO_API_PORT`        | `8080`           | Host port for the API.                           |
| `ANSEO_WEB_PORT`        | `5173`           | Host port for the web dashboard.                 |
| `POSTGRES_USER`         | `anseo`          | Postgres role and component of `DATABASE_URL`.   |
| `POSTGRES_PASSWORD`     | `anseo`          | Postgres password and component of `DATABASE_URL`.|
| `POSTGRES_DB`           | `anseo`          | Postgres database name.                          |
| `RUST_LOG`              | `info`           | Tracing level for api + worker.                  |
| `ANSEO_API_IMAGE`       | `anseo/api:dev`  | Override to pull a published api image.          |
| `ANSEO_WORKER_IMAGE`    | `anseo/worker:dev` | Override to pull a published worker image.     |
| `ANSEO_WEB_IMAGE`       | `anseo/web:dev`  | Override to pull a published web image.          |

The `api` and `worker` services compose their `DATABASE_URL` and `REDIS_URL` from the variables above. The `web` service receives `ANSEO_API_BASE_URL=http://api:8080` for in-cluster API calls.

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
