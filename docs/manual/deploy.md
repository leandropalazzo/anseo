# Deploy manual

One multi-project core, **three deployment tiers**. They share the same engine, config (`anseo.yaml`), and data model — you pick a tier by how much you want running, and you can move up a tier later without changing your project.

| Tier | What runs | Reach for it when… |
|---|---|---|
| **0 — solo CLI** | just `anseo`, on demand | ad-hoc analysis, CI gates, scripting |
| **1 — single binary** | `anseo serve` (API + worker, one process) | an always-on single node, no Docker |
| **2 — Docker Compose** | API + worker + web + Postgres, as containers | the full stack incl. the dashboard |

Migrations apply automatically on first connect in every tier, so there is no separate "migrate" step.

---

## Tier 0 — solo CLI

No long-running services. You point `anseo` at a Postgres and invoke it when you need it. (`ogeo` remains a deprecated alias for the `anseo` CLI.)

```bash
export DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo
anseo init                                  # scaffold anseo.yaml
anseo login openai                          # store a provider key
anseo prompt run                            # run prompts × providers, extract + persist
anseo report generate --format markdown
anseo check visibility --expect-rank-lte 3  # exit-code gate for CI
```

Everything is one-shot. Nothing listens on a port; nothing runs between invocations. This is also the tier CI uses.

---

## Tier 1 — single binary (`anseo serve`)

One process runs the REST `/v1` API **and** the background worker (schedules, anomaly detection, alert/webhook delivery) in-process — no Compose, no separate worker.

```bash
anseo serve                                 # API + worker on 127.0.0.1:8080
anseo serve --port 9000                     # change the port
anseo serve --projects-dir /srv/anseo              # project dir holding anseo.yaml (default: cwd)
anseo serve --bind 0.0.0.0:8080             # non-loopback bind — read the warning below
```

**Postgres — two modes, chosen automatically:**

- **No `DATABASE_URL`** → `anseo serve` provisions and supervises a **managed child Postgres** for the lifetime of the process (and stops it cleanly on shutdown). Nothing else to install — this is the zero-dependency path.
- **`DATABASE_URL` set** → it uses your external Postgres unchanged.

```bash
DATABASE_URL=postgres://anseo:anseo@db.internal:5432/anseo anseo serve
```

**Binding & safety.** The default bind is `127.0.0.1:8080` (localhost-only). A **non-loopback bind with no API keys configured is refused** — so you can't accidentally expose an unauthenticated instance. Before binding to a public interface: create API keys (`anseo api key create`) and put the process behind your own TLS / auth / network controls.

> **Standalone exposure baseline (Tier 2).** Before you set `ANSEO_BIND_HOST=0.0.0.0`:
> 1. **Rotate the bootstrap key — in the database, not just `.env`.** The shipped `ANSEO_BOOTSTRAP_API_KEY` is a well-known dev credential, seeded into Postgres on first boot (only when zero keys exist) under the name `bootstrap`. If you already booted the trial with the default, changing the env var does **not** revoke the persisted key. Mint a fresh named key, point `.env` at it, then revoke `bootstrap`:
>    The `anseo` CLI is **not** bundled in the runtime images, so run it from a
>    host install (cargo-installed or a release binary) against the stack's
>    published Postgres (`127.0.0.1:5432`) and the same project config. Match the
>    credentials to your `.env`:
>    ```bash
>    export DATABASE_URL="postgres://anseo:anseo@127.0.0.1:5432/anseo"   # match .env
>    # a) Mint a new key — the plaintext is printed ONCE; copy it.
>    anseo api key create --name prod --config ./anseo.example.yaml
>    # b) Set ANSEO_BOOTSTRAP_API_KEY to that plaintext (web/SSR + healthchecks
>    #    authenticate with it), then recreate the api/web containers.
>    $EDITOR .env && docker compose up -d
>    # c) Revoke the well-known dev key (by name; idempotent).
>    anseo api key revoke --name bootstrap --config ./anseo.example.yaml
>    ```
>    Setting a strong `ANSEO_BOOTSTRAP_API_KEY` *before the first `up`* seeds that value instead and avoids the rotation entirely.
> 2. **Rotate `POSTGRES_PASSWORD` and `ANSEO_KEYRING_PASSPHRASE`.** Keep the password URL-safe, or set a percent-encoded `DATABASE_URL` directly (see `.env.example`).
> 3. **Datastores stay localhost.** Postgres/Redis publish on `127.0.0.1` regardless of `ANSEO_BIND_HOST`; `0.0.0.0` only opens the api/web ports. Keep it that way — Redis has no auth.
>
> This is **enforced**, not just advised: a `preflight` guard runs before api/worker/web and **refuses to start the stack** if `ANSEO_BIND_HOST` is non-loopback while `ANSEO_BOOTSTRAP_API_KEY`, `ANSEO_KEYRING_PASSPHRASE`, or `POSTGRES_PASSWORD` is still the shipped dev default or unset. A localhost trial is unaffected. (Rotating the env still requires step 1's in-DB revoke if you already booted with the default key.)

---

## Tier 2 — Docker Compose

The full containerized stack — API, worker, web dashboard, and Postgres — from `infra/docker`.

```bash
cp infra/docker/.env.example infra/docker/.env   # defaults are fine for a local run
docker compose -f infra/docker/compose.yml up -d
docker compose -f infra/docker/compose.yml ps    # wait until services are healthy
docker compose -f infra/docker/compose.yml down  # tear down
```

**Localhost-only by default.** Every published port binds to `127.0.0.1`. Override the interface with the `ANSEO_BIND_HOST` env var **only** once the instance is behind your own network controls. Other knobs (`POSTGRES_PORT`, credentials, …) live in `.env` with sensible defaults baked into `compose.yml`.

> Working from a source checkout? `scripts/local-deploy.sh` wraps build → up → health-wait and rebuilds only stale app images.

### Tier 2, standalone — production, no source checkout

For a production / self-host deploy you don't need to clone the repo at all. A single standalone `compose.yml` pulls the **published, version-pinned** GHCR images and wires the same stack (Postgres + Redis + api + worker + web):

```bash
curl -fsSL https://anseo.ai/compose.yml        -o compose.yml
curl -fsSL https://anseo.ai/.env.example       -o .env
curl -fsSL https://anseo.ai/anseo.example.yaml -o anseo.example.yaml   # required: bind-mounted into api + worker

# Set ANSEO_VERSION, rotate every "CHANGE THIS" secret, and edit
# anseo.example.yaml (brand, prompts, providers) before exposing.
$EDITOR .env anseo.example.yaml

docker compose up -d
docker compose ps        # wait until every service is (healthy)
```

Versioned snapshots are served too (e.g. `https://anseo.ai/compose/0.5.0.yml`) so a deploy can pin an exact bundle. Same localhost-only default (`ANSEO_BIND_HOST`); the artifact source lives at [`infra/standalone/`](../../infra/standalone/README.md). Before exposing to the internet, follow the exposure baseline below.

---

## After it's up (any tier)

- **Analytics (ClickHouse).** Optional and connected **separately** — use the dashboard's guided setup (Tier 2) or `anseo analytics migrate-to-clickhouse`. The core loop works on Postgres alone.
- **Scheduling & alerts.** Declared in `anseo.yaml` and executed by the worker — in-process on Tier 1, the worker container on Tier 2. See the [CLI manual](./cli.md) (`anseo schedule`, `anseo webhook`).
- **Health.** Tier 1: the API answers on its bind address. Tier 2: `docker compose ps` reports every service healthy.
- **Moving up a tier.** Point a higher tier at the same `DATABASE_URL` and it picks up your existing projects and history — the config and data model don't change between tiers.
