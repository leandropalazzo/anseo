# Anseo — standalone production stack

This directory holds the **self-host deploy bundle**: `compose.yml`,
`.env.example`, and `anseo.example.yaml` bring the whole Anseo stack up from
**published GHCR images**, with **no source checkout required**.

These two files are what gets served from the stable public URLs:

| URL | File |
| --- | --- |
| `https://anseo.ai/compose.yml`  | `infra/standalone/compose.yml` |
| `https://anseo.ai/.env.example` | `infra/standalone/.env.example` |
| `https://anseo.ai/anseo.example.yaml` | `infra/standalone/anseo.example.yaml` |
| `https://anseo.ai/compose/X.Y.Z.yml` | a pinned snapshot of `compose.yml` |

> This is distinct from `infra/docker/compose.yml`, which is the **dev** stack
> (it builds images from a local source checkout). Use *this* directory for
> production / self-host.

## Bring-up (clean host, no repo clone)

On any host with Docker + the Compose plugin:

```bash
curl -fsSL https://anseo.ai/compose.yml  -o compose.yml
curl -fsSL https://anseo.ai/.env.example -o .env
curl -fsSL https://anseo.ai/anseo.example.yaml -o anseo.example.yaml

# Edit .env — at minimum set ANSEO_VERSION, and rotate every "CHANGE THIS" secret
# before exposing the stack to a network. Edit anseo.example.yaml to set your
# brand, prompts, competitors, and providers.
$EDITOR .env
$EDITOR anseo.example.yaml

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
| `ANSEO_VERSION` | **Required, no default.** Pinned release tag (`X.Y.Z`) for the GHCR images — must be a real [published release](https://github.com/leandropalazzo/opengeo/releases). Never `latest`/`dev`. The stack fails fast if unset. | _(unset)_ |
| `ANSEO_IMAGE_REGISTRY` | Registry + repo prefix; images resolve to `<prefix>/{api,worker,web}:<ANSEO_VERSION>`. | `ghcr.io/leandropalazzo/opengeo` |
| `ANSEO_PROJECT_CONFIG` | Host path to the canonical Anseo YAML mounted read-only into api + worker at `/anseo.yaml`. | `./anseo.example.yaml` |
| `ANSEO_CONFIG` | In-container path the api + worker read. Override only if you also mount a file at that path. | `/anseo.yaml` |
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
- **No source mounts.** Unlike the dev stack, nothing here bind-mounts source.
  The only bind mount is the read-only project config at `/anseo.yaml`.
- **Secrets via env.** All secrets come from `.env`; none are baked into the image.

## Project config

The API and worker both require a readable Anseo YAML file. Without it, the API
can report healthy while live prompt dispatch returns `orchestrator_unconfigured`
and the worker disables scheduled dispatch.

Start from `anseo.example.yaml` and edit:

- `schema_version` — `0.1` for the standalone example schema.
- `brand.name`, `brand.variants`, and `competitors` — entities to monitor.
- `prompts[].name`, `prompts[].text`, and optional `prompts[].description`.
- `providers[].name`, optional `providers[].model`, optional
  `providers[].models` for OpenRouter, and optional `providers[].timeout_seconds`.
- `concurrency` — optional prompt-run parallelism, default `4`.

Provider API keys do not belong in `anseo.yaml`; set them in the dashboard or
through the supported secret-store flow. Declared providers with no key produce
failed prompt-run records that explain which key is missing instead of making
the orchestrator unavailable.

For a less example-named deployment file, copy it to `anseo.yaml` and set
`ANSEO_PROJECT_CONFIG=./anseo.yaml` in `.env`.

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
pinned to `X.Y.Z`, datastores pinned, all ports `127.0.0.1`, and the default
project config mounted read-only into api + worker):

```bash
./infra/standalone/validate-compose.sh
```
