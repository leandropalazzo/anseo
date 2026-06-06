# Anseo First-Party Plugins

This directory holds the source and bundle layout for the **official, first-party
Anseo plugins** (Story 41.5). They are the canonical reference implementations of
the four Phase-3 plugin kinds and double as the SDK author templates.

Two plugins seed the registry. Every one is **fully functional within the
host execution contract** — none is a stub.

| Directory                  | id (registry)                  | kind            | What it actually does                                            |
| -------------------------- | ------------------------------ | --------------- | --------------------------------------------------------------- |
| `anseo-trend-analytics/`   | `anseo/anseo-trend-analytics`  | `analytics`     | Offline rollup (count/min/max/mean/least-squares slope) over a metric series; emits a `plugin:anseo-trend-analytics:rollup` trend result to stdout. Native subprocess — the supported, platform-gated, tested kind. |
| `anseo-example-provider/`  | `anseo/anseo-example-provider` | `provider`      | Deterministic OFFLINE echo provider — the WASM plugin-author template. |

### Why no `output-format` reference plugin?

An `output-format` reference plugin is **deferred** until the output-format
execution model is settled. The plugin contract documents `output-format` as a
**WASM** kind, but no WASM execution runtime is wired in the host today — the
only execution primitive that exists is the **native analytics subprocess**
(`crates/plugin-host/src/subprocess.rs`). Shipping an output-format plugin now
would force a choice between mis-modeling it as native (which regresses the
loader's platform gate — it would wrongly skip legitimate WASM output-format
plugins on Windows) or shipping a manifest the host cannot actually execute
(a parity-honesty defect). Once the output-format runtime (WASM host vs native)
is decided, the reference plugin lands against the real contract.

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

## What is (and isn't) in git

This directory holds plugin **SOURCE only**:

```text
<plugin>/
  manifest.yaml      # PluginManifest — the on-disk schema (crates/plugin-manifest)
  Cargo.toml         # standalone workspace (empty [workspace] table)
  src/               # plugin source
```

The built `entrypoint.wasm` artifacts are **intentionally NOT committed** — no
binaries in version control. The manifests reference `entrypoint.wasm` because
that is the install-layout filename the runtime resolves; the artifact is
produced by the build step below, never stored in git. `plugins/dist/` (the
default build output) is gitignored.

## Materializing installable bundles

Run the build script to compile each plugin and lay it into the **exact**
registry / install layout the host reads:

```bash
plugins/build.sh            # → plugins/dist/ (default), idempotent
plugins/build.sh /tmp/out   # → custom output dir
```

It reads `id` + `version` from each `manifest.yaml`, compiles the plugin, and
stages the artifact + manifest as:

```text
<out>/plugins/<id>/<version>/manifest.yaml
<out>/plugins/<id>/<version>/entrypoint.wasm
```

This is the same shape resolved by the loader, the registry client
(`crates/plugin-host/src/registry.rs` → `plugins/<id>/<version>/entrypoint.wasm`),
and the CLI installer (`apps/cli/src/commands/plugin_install.rs`). `<id>` is the
namespaced registry id (`anseo/<name>`).

Build kinds:

- **`anseo-trend-analytics`** (analytics) — the supported **native subprocess**
  kind. Built with `cargo build --release`; the release binary is staged as
  `entrypoint.wasm` (the host's subprocess adapter spawns whatever lives at that
  path — see `crates/plugin-host/src/subprocess.rs`; the filename is the install
  convention, not the artifact format).
- **`anseo-example-provider`** (provider) — the WASM cdylib kind. Built with
  `cargo build --release --target wasm32-wasip1`. If that rustup target isn't
  installed the script prints the install command (`rustup target add
  wasm32-wasip1`) and **skips** that bundle rather than failing the whole run, so
  the native plugin still materializes.

### Signing → signed, installable bundles

The build script materializes the **unsigned** layout. To produce signed,
installable bundles, run the **41.4 / 38.19 release+signing pipeline** on top of
the materialized output. It computes `SHA-256(manifest.yaml || entrypoint.wasm)`,
signs it with the namespace author key (Ed25519), and emits the signed-bundle
siblings — these are generated + published to `github.com/anseo/plugin-registry`,
not checked in here:

```text
plugins/<id>/<version>/manifest.yaml
plugins/<id>/<version>/entrypoint.wasm
plugins/<id>/<version>/signature.bin       # 64-byte Ed25519 over SHA-256(manifest.yaml || entrypoint.wasm)
plugins/<id>/<version>/claim.toml          # namespace claim + root signature
```

See `crates/plugin-host/src/signing.rs` for the Ed25519 + TOFU signing /
verification chain (`signing_digest`, `NamespaceClaim`) and
`docs/manual/plugin-authoring.md` for the publisher flow. Unsigned bundles load
only behind `--allow-unsigned` (CLI) / `LoadPolicy::allow_unsigned`.

> **Deferred host hardening.** At runtime `scan_and_load` decides load/skip/error
> from the manifest, the recorded `signature_status`, and the platform sandbox
> capability — it does **not** yet verify that `entrypoint.wasm` is present on
> disk or recompute/verify the Ed25519 signature over the bundle bytes at load
> time. That load-time artifact-presence + signature verification is tracked
> separately as part of the subprocess/loader hardening follow-up. (The
> first-party load-roundtrip test still stages a real built `entrypoint.wasm`
> next to each manifest, so it stays high-fidelity rather than passing for a
> bundle missing its entrypoint.)

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
