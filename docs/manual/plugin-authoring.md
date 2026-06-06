# Authoring & Publishing Anseo Plugins

Anseo plugins extend the pipeline through a small, capability-scoped SDK. A
plugin is one of four kinds — `provider`, `extractor`, `analytics`, or
`output-format` — packaged as a WASM artifact plus a manifest, and published to
a GitHub flat-file registry. The host (`anseo serve` / the worker) loads
installed plugins; the dashboard `/marketplace`, the `anseo plugin` CLI, and the
MCP `list_plugins` / `install_plugin` tools all read the same live registry.

This guide is a starting point: it covers the `plugin.yaml` manifest and how to
publish a version to a registry. The full capability catalog and the signing
flow are linked at the end.

## 1. Write a `plugin.yaml` manifest

Every plugin declares itself in a `plugin.yaml`. The manifest is pure data — no
code runs to validate it — so you can check it locally before building anything:

```bash
anseo plugin validate ./plugin.yaml
```

A minimal manifest:

```yaml
# plugin.yaml
name: acme/serp-enrichment        # namespace/name — your registry id
version: 0.1.0                    # explicit, pinned (never "latest")
author: Acme Labs
homepage: https://github.com/acme/serp-enrichment
plugin_type: extractor           # provider | extractor | analytics | output-format

# Capabilities are a CLOSED catalog. Declare only what you use — the host
# enforces them at runtime and the operator sees them before install.
capabilities:
  - kind: network
    allowlist:
      - api.serpprovider.com
  - kind: read-secret
    keys:
      - SERP_API_KEY
```

Rules the validator enforces:

- `name` is `namespace/name`; the namespace is what you claim and sign under.
- `version` is explicit (no implicit `latest`).
- `plugin_type` is exactly one of the four kinds.
- Every capability is a member of the closed catalog (see the SDK reference).
  Plugins **cannot** mint new MCP tools, Web routes, or CLI verbs — they reach
  users through existing surfaces (e.g. plugin trend kinds flow through
  `list_trends`).

## 2. Build the artifact

Compile your plugin to a single WASM entrypoint (`entrypoint.wasm`). The host
loads this artifact in a sandbox scoped to the capabilities you declared.

## 3. Publish to a registry

The registry is a plain directory tree served over a CDN (GitHub raw is the
default) or any HTTP base URL. The on-disk layout per version:

```text
<registry>/index.toml                                   # registry root index
<registry>/plugins/<id>/<version>/manifest.yaml         # your plugin.yaml
<registry>/plugins/<id>/<version>/entrypoint.wasm       # the artifact
<registry>/plugins/<id>/<version>/signature.bin         # 64-byte Ed25519 sig
<registry>/plugins/<id>/<version>/claim.toml            # namespace claim + sig
```

Add a row to `index.toml` for the new version, including the SHA-256 of the
artifact (the client refuses an artifact whose digest does not match):

```toml
[[plugin]]
id = "acme/serp-enrichment"
version = "0.1.0"
description = "Enrich runs with live SERP snippets for citation grounding."
sha256 = "<lowercase hex sha-256 of entrypoint.wasm>"
yanked = false
```

To publish to the **canonical** community registry, open a PR against
`anseo/plugin-registry`. To serve your own (a fork or an internal registry),
point clients at it with:

```bash
export ANSEO_PLUGIN_REGISTRY_URL="https://raw.githubusercontent.com/acme/plugin-registry/main"
anseo plugin search serp
anseo plugin install acme/serp-enrichment
```

## 4. Sign your release (recommended)

Unsigned plugins install only behind an explicit acknowledgment
(`--allow-unsigned` on the CLI, the ⚠ confirmation dialog in the dashboard).
Signed plugins carry a root-countersigned namespace `claim.toml` and a per-
artifact `signature.bin`, and install cleanly with a verified-publisher badge.
The signing pipeline (key generation, namespace claims, rotation, revocation)
is documented in the SDK reference below.

## See also

- Plugin SDK reference — the closed capability catalog, the WASM host contract,
  and the §5.4 Ed25519 + TOFU signing/verification chain.
- `anseo plugin --help` — `search`, `install`, `list`, `remove`, `upgrade`.
- The dashboard `/marketplace` page — browse, inspect, and install from the
  same live registry.
