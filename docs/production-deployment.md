# Production deployment guide

Anseo defaults to `127.0.0.1` (loopback) on every surface — the stack is designed to be safe by default and only reachable from the same host. This guide covers everything you need to expose it safely when you want your team (or the internet) to reach it.

> **Rule (non-negotiable):** Do **not** expose Anseo to a public network without a reverse proxy, TLS, and auth in front of it. The OSS stack has no built-in authentication for the web dashboard or MCP surfaces; only the `/v1` API enforces per-project API keys.

---

## Why a reverse proxy is required

| Surface | Auth gate | Safe to expose directly? |
|---------|-----------|--------------------------|
| REST `/v1` | Per-project API key (`Authorization: Bearer …`) | No — use a proxy for TLS + rate-limit |
| Web dashboard (`/`) | None (OSS) | **No** — proxy must enforce auth |
| MCP server (`/mcp`) | None (OSS) | **No** — proxy must enforce auth, or keep on localhost/VPN |
| `/healthz` | None | Acceptable to expose read-only through the proxy |

The proxy is the auth boundary for the web and MCP surfaces. Keep them on `localhost` or behind a VPN if you cannot enforce auth at the proxy level.

---

## Expose safely — two copy-paste configs

### Option A: Caddy (recommended — automatic TLS via Let's Encrypt)

Install [Caddy](https://caddyserver.com/) then drop this into `/etc/caddy/Caddyfile`:

```caddyfile
anseo.example.com {
    # Terminate TLS (auto-provisioned via Let's Encrypt) and forward to the
    # local stack. Adjust the port to match your --port / ANSEO_API_PORT setting.
    reverse_proxy localhost:8080

    # Basic auth for every non-API surface: dashboard, MCP, assets, and routes.
    @protected {
        not path /v1* /healthz
    }
    # Generate a hash with: caddy hash-password
    basicauth @protected {
        your_username <bcrypt-hash-here>
    }

    # Optional: restrict dashboard/MCP to your team's IP range.
    # @internal remote_ip 10.0.0.0/8
    # handle @internal { reverse_proxy localhost:8080 }
}
```

Start Caddy and the stack:

```bash
# Run the Anseo stack (API + worker, managed Postgres) bound to localhost.
anseo serve --port 8080

# In another terminal (or as a systemd unit):
caddy run --config /etc/caddy/Caddyfile
```

Caddy handles HTTPS certificate provisioning and renewal automatically.

One-liner for a quick test (no Caddyfile required):

```bash
caddy reverse-proxy --from anseo.example.com --to localhost:8080
```

---

### Option B: nginx (manual TLS)

Obtain a certificate (e.g. `certbot --nginx -d anseo.example.com`) then add a server block:

```nginx
# /etc/nginx/sites-available/anseo
server {
    listen 80;
    server_name anseo.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name anseo.example.com;

    ssl_certificate     /etc/letsencrypt/live/anseo.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/anseo.example.com/privkey.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;

    # Enforce auth on the dashboard and MCP surfaces.
    # Create with: htpasswd -c /etc/nginx/.htpasswd your_username
    location / {
        auth_basic           "Anseo";
        auth_basic_user_file /etc/nginx/.htpasswd;

        proxy_pass         http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header   Host              $host;
        proxy_set_header   X-Real-IP         $remote_addr;
        proxy_set_header   X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;
        proxy_set_header   Upgrade           $http_upgrade;
        proxy_set_header   Connection        "upgrade";
    }

    # /v1 is already API-key-gated; basic-auth is optional but recommended
    # for defense-in-depth.
    location /v1/ {
        proxy_pass         http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header   Host              $host;
        proxy_set_header   X-Real-IP         $remote_addr;
        proxy_set_header   X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;
    }

    # Expose /healthz without auth so your load balancer / uptime monitor
    # can reach it.
    location /healthz {
        proxy_pass http://127.0.0.1:8080/healthz;
    }
}
```

Enable and reload:

```bash
ln -s /etc/nginx/sites-available/anseo /etc/nginx/sites-enabled/
nginx -t && systemctl reload nginx
```

---

## Docker Compose stack

When running Tier 2 (Docker Compose), set `ANSEO_BIND_HOST=127.0.0.1` (the default) in `infra/docker/.env` and let Caddy or nginx terminate TLS externally. **Do not** publish the stack ports directly to `0.0.0.0` unless they are already behind a firewall.

```bash
# infra/docker/.env
ANSEO_BIND_HOST=127.0.0.1   # default — safe; override only behind a proxy
ANSEO_API_PORT=8080
```

---

## Production checklist

Before exposing Anseo to any network beyond localhost:

- [ ] **Pinned container images** — use a version tag (e.g. `ghcr.io/leandropalazzo/anseo/api:v0.6.0`), not `latest`, so deployments are reproducible and rollbacks are clean.
- [ ] **Reverse proxy + TLS** — Caddy or nginx in front, with a valid certificate. Direct port exposure to the internet is not supported.
- [ ] **API-key gate enabled** — create at least one project key with `anseo api key create --name prod`; the `/v1` surface requires it. Document the key rotation process for your team.
- [ ] **Secrets injected, not baked** — pass `DATABASE_URL`, provider keys, and `ANSEO_API_KEY_*` via environment variables or a secrets manager; do not hard-code them in compose files or container images.
- [ ] **Postgres backups scheduled** — set up `pg_dump` (or equivalent) with off-host storage and verified restores. Anseo stores all benchmark data and brand configs in Postgres.

---

## Minimal API-key gate

The `/v1` REST surface requires a project-scoped API key on every request:

```bash
# Create a key (shown once in plaintext):
anseo api key create --name prod

# Use it:
curl -H "Authorization: Bearer ogeo_…" https://anseo.example.com/v1/projects
```

The web dashboard and MCP server are **not** key-gated in the OSS stack. Use the proxy-layer auth (`basicauth` / `auth_basic`) shown above, or keep those surfaces on `localhost` / a VPN-only interface.

---

## Non-loopback bind warning

If you start `anseo serve --bind 0.0.0.0:8080` (or set `ANSEO_BIND_HOST=0.0.0.0`) without a proxy in front, the CLI prints a startup warning:

```
⚠️  WARNING: binding to 0.0.0.0:8080 exposes Anseo on a non-loopback interface.
   Anseo OSS has no built-in auth for the web or MCP surfaces.
   Ensure a reverse proxy (Caddy, nginx) with TLS and auth is in front.
   See docs/production-deployment.md for copy-paste Caddy/nginx configs.
```

This is non-blocking — it is your signal to double-check the proxy is in place.
