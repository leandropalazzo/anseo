# Anseo plugin registry (seed)

This directory is the **seed content** for the public, GitHub-hosted Anseo
plugin registry: `github.com/anseo/plugin-registry`. The live registry is just a
flat-file tree served over GitHub's raw CDN — there is no registry server.

`anseo plugin search` / `anseo plugin install` read the tree through the
transport-agnostic client in `crates/plugin-host/src/registry.rs`. The default
base URL (declared once, in `DEFAULT_REGISTRY_URL`) is:

```
https://raw.githubusercontent.com/anseo/plugin-registry/main
```

Override it with `ANSEO_PLUGIN_REGISTRY_URL` (e.g. to point at a fork or a local
`http://` mirror).

## Layout

```text
index.toml                                   # registry root index (search)
plugins/<id>/<version>/manifest.yaml         # PluginManifest (YAML)
plugins/<id>/<version>/entrypoint.wasm       # artifact bytes
plugins/<id>/<version>/signature.bin         # 64-byte Ed25519 author signature
plugins/<id>/<version>/claim.toml            # namespace claim + root signature
keys/revoked.toml                            # key/plugin revocation list
```

## Verification (enforced by the client, not the registry)

1. **Integrity** — the client recomputes the SHA-256 of `entrypoint.wasm` and
   rejects any artifact whose digest does not match the `sha256` in `index.toml`
   (`error: integrity check failed`).
2. **Authenticity** — the client runs the Ed25519 + TOFU chain against the
   compile-pinned root key(s) and the per-namespace pinned author key.
3. **Unsigned plugins** — install requires an explicit `--allow-unsigned`, which
   records `signature_status = unsigned` and prints a prominent
   `[UNSIGNED PLUGIN]` warning.

## Caching

The fetched `index.toml` is cached for **1 hour** under the plugin home
(`<plugin_home>/cache/index.toml`). Newly published plugins may not appear until
the TTL expires; pass `--refresh` to `anseo plugin search` to bust the cache.

## Publishing the live repo

To bootstrap `github.com/anseo/plugin-registry`, push the contents of this
directory to the repo root on the `main` branch. The seed `index.toml` is empty;
first-party plugins are added by Story 41.5.
