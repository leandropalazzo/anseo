# Deploy manual

One multi-project core, **three deployment tiers**. They share the same engine, config (`opengeo.yaml`), and data model — you pick a tier by how much you want running, and you can move up a tier later without changing your project.

| Tier | What runs | Reach for it when… |
|---|---|---|
| **0 — solo CLI** | just `ogeo`, on demand | ad-hoc analysis, CI gates, scripting |
| **1 — single binary** | `ogeo serve` (API + worker, one process) | an always-on single node, no Docker |
| **2 — Docker Compose** | API + worker + web + Postgres, as containers | the full stack incl. the dashboard |

Migrations apply automatically on first connect in every tier, so there is no separate "migrate" step.

---

## Tier 0 — solo CLI

No long-running services. You point `ogeo` at a Postgres and invoke it when you need it.

```bash
export DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo
ogeo init                                  # scaffold opengeo.yaml
ogeo login openai                          # store a provider key
ogeo prompt run                            # run prompts × providers, extract + persist
ogeo report generate --format markdown
ogeo check visibility --expect-rank-lte 3  # exit-code gate for CI
```

Everything is one-shot. Nothing listens on a port; nothing runs between invocations. This is also the tier CI uses.

---

## Tier 1 — single binary (`ogeo serve`)

One process runs the REST `/v1` API **and** the background worker (schedules, anomaly detection, alert/webhook delivery) in-process — no Compose, no separate worker.

```bash
ogeo serve                                 # API + worker on 127.0.0.1:8080
ogeo serve --port 9000                     # change the port
ogeo serve --dir /srv/opengeo              # project dir holding opengeo.yaml (default: cwd)
ogeo serve --bind 0.0.0.0:8080             # non-loopback bind — read the warning below
```

**Postgres — two modes, chosen automatically:**

- **No `DATABASE_URL`** → `ogeo serve` provisions and supervises a **managed child Postgres** for the lifetime of the process (and stops it cleanly on shutdown). Nothing else to install — this is the zero-dependency path.
- **`DATABASE_URL` set** → it uses your external Postgres unchanged.

```bash
DATABASE_URL=postgres://opengeo:opengeo@db.internal:5432/opengeo ogeo serve
```

**Binding & safety.** The default bind is `127.0.0.1:8080` (localhost-only). A **non-loopback bind with no API keys configured is refused** — so you can't accidentally expose an unauthenticated instance. Before binding to a public interface: create API keys (`ogeo api key create`) and put the process behind your own TLS / auth / network controls.

---

## Tier 2 — Docker Compose

The full containerized stack — API, worker, web dashboard, and Postgres — from `infra/docker`.

```bash
cp infra/docker/.env.example infra/docker/.env   # defaults are fine for a local run
docker compose -f infra/docker/compose.yml up -d
docker compose -f infra/docker/compose.yml ps    # wait until services are healthy
docker compose -f infra/docker/compose.yml down  # tear down
```

**Localhost-only by default.** Every published port binds to `127.0.0.1`. Override the interface with the `OGEO_BIND_HOST` env var **only** once the instance is behind your own network controls. Other knobs (`POSTGRES_PORT`, credentials, …) live in `.env` with sensible defaults baked into `compose.yml`.

> Working from a source checkout? `scripts/local-deploy.sh` wraps build → up → health-wait and rebuilds only stale app images.

---

## After it's up (any tier)

- **Analytics (ClickHouse).** Optional and connected **separately** — use the dashboard's guided setup (Tier 2) or `ogeo analytics migrate-to-clickhouse`. The core loop works on Postgres alone.
- **Scheduling & alerts.** Declared in `opengeo.yaml` and executed by the worker — in-process on Tier 1, the worker container on Tier 2. See the [CLI manual](./cli.md) (`ogeo schedule`, `ogeo webhook`).
- **Health.** Tier 1: the API answers on its bind address. Tier 2: `docker compose ps` reports every service healthy.
- **Moving up a tier.** Point a higher tier at the same `DATABASE_URL` and it picks up your existing projects and history — the config and data model don't change between tiers.
