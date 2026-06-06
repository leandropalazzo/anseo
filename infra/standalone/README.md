# Anseo — standalone production stack

This directory holds the **self-host deploy bundle**: a single `compose.yml` plus
`.env.example` that bring the whole Anseo stack up from **published GHCR images**,
with **no source checkout required**.

These two files are what gets served from the stable public URLs:

| URL | File |
| --- | --- |
| `https://anseo.ai/compose.yml`  | `infra/standalone/compose.yml` |
| `https://anseo.ai/.env.example` | `infra/standalone/.env.example` |
| `https://anseo.ai/compose/vX.Y.Z.yml` | a pinned snapshot of `compose.yml` |

> This is distinct from `infra/docker/compose.yml`, which is the **dev** stack
> (it builds images from a local source checkout). Use *this* directory for
> production / self-host.

## Bring-up (clean host, no repo clone)

On any host with Docker + the Compose plugin:

```bash
curl -fsSL https://anseo.ai/compose.yml  -o compose.yml
curl -fsSL https://anseo.ai/.env.example -o .env

# Edit .env — at minimum set ANSEO_VERSION, and rotate every "CHANGE THIS" secret
# before exposing the stack to a network.
$EDITOR .env

docker compose up -d
docker compose ps        # wait for every service to report (healthy)
```

The stack:

- `postgres` (`postgres:16-alpine`) + `redis` (`redis:7-alpine`) — datastores
- `api` — the Axum API (runs DB migrations automatically on boot)
- `worker` — background schedule/orchestrator worker
- `web` — the Next.js dashboard (default `http://127.0.0.1:5173`)

The API serves on `http://127.0.0.1:8080`. Reach it / the dashboard with the
`ANSEO_BOOTSTRAP_API_KEY` from your `.env`.

## Configuration (`.env`)

| Var | Purpose | Default |
| --- | --- | --- |
| `ANSEO_VERSION` | **Required.** Pinned release tag (`vX.Y.Z`) for the GHCR images. Never `latest`/`dev`. | `v0.5.0` |
| `ANSEO_IMAGE_REGISTRY` | Registry + repo prefix; images resolve to `<prefix>/{api,worker,web}:<ANSEO_VERSION>`. | `ghcr.io/leandropalazzo/opengeo` |
| `ANSEO_BIND_HOST` | Host interface for published ports. `0.0.0.0` only behind a proxy you control. | `127.0.0.1` |
| `ANSEO_BOOTSTRAP_API_KEY` | API key seeded on first boot (dashboard + healthchecks). **CHANGE THIS.** | dev key |
| `ANSEO_KEYRING_PASSPHRASE` | Unlocks the age-encrypted provider-secrets file. **CHANGE THIS.** | dev passphrase |
| `POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB` | Postgres credentials. **CHANGE THIS.** | `anseo` |
| `POSTGRES_PORT` / `REDIS_PORT` / `ANSEO_API_PORT` / `ANSEO_WEB_PORT` | Host port overrides. | `5432` / `6379` / `8080` / `5173` |
| `RUST_LOG` | Log level for api + worker. | `info` |

> Copied **unedited**, `.env.example` boots a localhost-only trial. Before
> exposing the stack to any network, rotate every value marked
> **"CHANGE THIS before exposing"**.

## Security defaults

- **Localhost-only.** Every published port binds `127.0.0.1` unless you set
  `ANSEO_BIND_HOST=0.0.0.0`.
- **No dev mounts.** Unlike the dev stack, nothing here bind-mounts source or
  fixtures; the only volumes are named data volumes.
- **Secrets via env.** All secrets come from `.env`; none are baked into the image.

Before exposing to the internet, follow the exposure baseline (reverse
proxy + TLS, auth hardening, backups): **https://anseo.ai/docs/deploy**
(also `docs/manual/deploy.md` in the repo).

## Persistence

Named volumes survive `docker compose down && docker compose up -d`:

- `postgres-data` — the database
- `redis-data` — Redis AOF
- `api-secrets` — the age-encrypted provider-secrets file (dashboard-set keys)

`docker compose down -v` deletes these — only use it to wipe state intentionally.

## Upgrades / rollback

Edit `ANSEO_VERSION` in `.env` to the new pinned tag, then:

```bash
docker compose pull
docker compose up -d
```

Migrations run automatically on API boot. Roll back by setting `ANSEO_VERSION`
to the previous tag and repeating.

## Validating the artifact

`validate-compose.sh` asserts the production shape (no `build:`, app images
pinned to `vX.Y.Z`, datastores pinned, all ports `127.0.0.1`):

```bash
./infra/standalone/validate-compose.sh
```
