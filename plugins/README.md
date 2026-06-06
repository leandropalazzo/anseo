# Anseo First-Party Plugins

This directory holds the source and bundle layout for the **official, first-party
Anseo plugins** (Story 41.5). They are the canonical reference implementations of
the four Phase-3 plugin kinds and double as the SDK author templates.

Three plugins seed the registry:

| Directory                  | id (registry)              | kind            | What it does                                            |
| -------------------------- | -------------------------- | --------------- | ------------------------------------------------------- |
| `anseo-warehouse/`         | `anseo/anseo-warehouse`    | `analytics`     | Wraps the ClickHouse warehouse/ETL as a subprocess plugin (ADR-006 Tier-2+). |
| `anseo-connect-bigquery/`  | `anseo/anseo-connect-bigquery` | `output-format` | Streams run results to BigQuery (`output:connect`).     |
| `anseo-example-provider/`  | `anseo/anseo-example-provider` | `provider`      | Minimal WASM provider that calls a public echo API — the plugin-author template. |

## On-disk bundle shape

Every plugin ships exactly the layout the host loader (`crates/plugin-host`)
and the registry client (`crates/plugin-host/src/registry.rs`) expect:

```text
<plugin>/
  manifest.yaml      # PluginManifest — the on-disk schema (crates/plugin-manifest)
  <entry_point>      # WASM module (provider) or subprocess binary (analytics / output)
  src/               # plugin source — built by the 41.4 CI signing pipeline
```

The registry/install layout adds the signed-bundle siblings, produced by the
41.4 CI pipeline — they are NOT checked in here (they are generated + published
to `github.com/anseo/plugin-registry`):

```text
plugins/<id>/<version>/manifest.yaml
plugins/<id>/<version>/entrypoint.wasm     # or the subprocess binary
plugins/<id>/<version>/signature.bin       # 64-byte Ed25519 over SHA-256(manifest.yaml || entrypoint)
plugins/<id>/<version>/claim.toml          # namespace claim + root signature
```

## Parity boundary

These plugins reach users only through the **existing** surfaces via the
`plugin:<id>:<kind>` namespace — no new MCP tools, Web routes, or CLI verbs.
See `docs/plugin-surface-boundary.md`.

## Manifest schema notes

- `name` is the bare, DNS-safe plugin name (no `/`, `:`; max 128 chars). The
  registry id is `anseo/<name>`.
- `capabilities` is the closed catalog from `crates/plugin-manifest`
  (`network`, `read-secret`, `emit-event`, `extractor-confidence-override`,
  `analytics-window`). It must be present; an explicit empty list (`[]`) marks a
  zero-surface plugin.
- `entry_point` is a relative path inside the bundle (no absolute paths, no `..`).
- `plugin_type` is one of `provider | extractor | analytics | output-format`.

Each manifest here passes `PluginManifest::validate()` and loads cleanly through
`anseo_plugin_host::loader::scan_and_load`; see the integration test in
`crates/plugin-host/tests/first_party_plugins.rs`.
