# Anseo First-Party Plugins

This directory holds the source and bundle layout for the **official, first-party
Anseo plugins** (Story 41.5). They are the canonical reference implementations of
the four Phase-3 plugin kinds and double as the SDK author templates.

Three plugins seed the registry. Every one is **fully functional within the
host execution contract** — none is a stub.

| Directory                  | id (registry)                  | kind            | What it actually does                                            |
| -------------------------- | ------------------------------ | --------------- | --------------------------------------------------------------- |
| `anseo-trend-analytics/`   | `anseo/anseo-trend-analytics`  | `analytics`     | Offline rollup (count/min/max/mean/least-squares slope) over a metric series; emits a `plugin:anseo-trend-analytics:rollup` trend result to stdout. |
| `anseo-ndjson-export/`     | `anseo/anseo-ndjson-export`    | `output-format` | Formats a run's result rows as newline-delimited JSON (NDJSON) on stdout. |
| `anseo-example-provider/`  | `anseo/anseo-example-provider` | `provider`      | Deterministic OFFLINE echo provider — the plugin-author template. |

### Why no ClickHouse / BigQuery network sinks?

The host's plugin execution primitive is the **analytics subprocess sandbox**
(`crates/plugin-host/src/subprocess.rs`): the child runs under seccomp-bpf
(Linux) / `sandbox-exec (deny network*)` (macOS), the allowlist **forbids
`socket`/`connect`**, the host passes the request as **args**, captures
**stdout**, and **discards stderr**. There is no stdin and no wired host
fetch-proxy (the `host:http/fetch` capability in `crates/plugin-host/src/capability.rs`
is a call-time *policy* check only — no execution path delivers proxied bytes to
a subprocess). So a network-dependent ClickHouse/BigQuery integration **cannot
run** as a plugin today. Rather than ship stubs whose manifests overstate the
artifact (a parity-honesty defect), the first-party set is built from work the
sandbox can actually do: offline computation that reads args and writes stdout.
The `anseo-example-provider` template shows where a real `network` capability +
host-mediated fetch *would* go if/when that path is wired.

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
