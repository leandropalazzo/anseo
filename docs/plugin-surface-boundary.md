# Plugin Surface Boundary

> **Story 41.6 — Parity Honesty.** This document states, plainly and without
> aspiration, *which surfaces a plugin can reach and which it cannot*. It exists
> so that plugin authors don't build a plugin expecting to mint a new MCP tool or
> dashboard page, and so the parity CI contract isn't read as flagging a
> deliberate design decision as a bug.

Anseo's core promise is **CLI ⇄ Web ⇄ MCP parity** — the same data and operations
reachable however you work. Plugins are the *one* place that promise is
deliberately bounded. Decision **L3** (`architecture-phase3-plugin-sdk.md` §2)
states it directly: **plugins reach users only through *existing* surfaces, via
namespaced identifiers — they cannot mint new MCP tools, new Web routes, or new
CLI verbs.** That is a design choice, not a gap, and it is the **one accepted
parity exception** in the whole system.

> **Naming note.** This doc uses the **Anseo** product name. The CLI binary
> still ships as `ogeo` (the rename to `anseo` is provisional, tracked under
> Epic 45). Runtime env vars and the `anseo serve` command in the codebase
> already use the Anseo form; commands below match the **shipped** binary.

---

## The accepted parity exceptions

The parity contract (`crates/wire-schema/tests/parity_contract.rs`) treats every
registered capability as a row that must be either *covered* on all three
surfaces (CLI, Web/API, MCP) or carry an explicit, annotated exception. Two
deliberate exceptions exist:

| # | Capability / surface | Present on | Deliberately absent from | Decision ref |
|---|---|---|---|---|
| 1 | `plugin_namespaced_passthrough` (registered capability) | MCP (existing tool) | CLI, Web/API | `L3 / AD-Phase3-PluginsCannotRegisterMcpTools` |
| 2 | Operator site-analytics dashboard (`/v1/analytics/site-overview`, `/v1/analytics/funnels`, web `/analytics`) | Web/API + Web dashboard | MCP, CLI | Story 47.4 — operator-internal operational data, not an agent-facing prompt tool |

**Exception 1** is a *registered* capability (`plugin_namespaced_passthrough` in
`crates/wire-schema/src/parity.rs`) with a machine-checked
`single_surface_exception`.

**Exception 2 (Story 47.4)** is a different shape: the operator analytics surface
is intentionally **not** added to the capability `REGISTRY` at all. It is
operator-internal observability of the public site's GTM funnel — data the
operator reads in their own dashboard, never a tool an LLM agent invokes — so it
is dashboard/API-only by design and has no MCP or CLI counterpart. `anseo mcp
tools` does not list any analytics tool (AC-7). Because it is not a registered
capability, the parity test does not flag its absence on MCP/CLI; this row is the
human-readable record of that decision.

This is enforced by the test
[`plugin_passthrough_is_a_recognized_exception`](../crates/wire-schema/tests/parity_contract.rs)
and declared in the capability registry at
[`crates/wire-schema/src/parity.rs`](../crates/wire-schema/src/parity.rs)
(`plugin_namespaced_passthrough`). The registry row records its own rationale, so
the contract and this document cannot silently drift: if someone changed the
exception, the test changes with it.

**Why an exception and not a fix?** Letting plugins register first-class MCP tools
or Web routes would mean third-party code defining agent-facing tool schemas and
shipping UI into the operator's dashboard. That is a substantially larger trust,
sandboxing, and surface-stability problem than the SDK is designed to take on in
Phase 3. Closing the exception (plugin-managed surfaces) is explicitly **out of
scope** for this story and would require Phase 4 design work.

---

## What plugins *can* do — the `plugin:<id>:<kind>` namespace

A plugin reaches users by contributing **artifacts** that flow through surfaces
that *already exist*, identified by the namespace convention
**`plugin:<id>:<kind>`**. There are four plugin types
(`crates/plugin-manifest`):

| Plugin type | What it contributes | Where the user sees it |
|---|---|---|
| `provider` | A new AI-search provider | Prompt runs — same run path as built-in providers (`crates/providers/src/plugin.rs`) |
| `extractor` | Brand/competitor extraction logic | Extraction stage of a run |
| `analytics` | A trend/metric kind | Verbatim through the **existing** `list_trends` MCP output |
| `output-format` | A new render format for existing data | Existing `--format` style output paths |

The key invariant: a plugin trend kind appears **inside** the existing
`list_trends` output, not as a new `list_plugin_trends` tool. A plugin provider
runs through the **existing** prompt-run path, not a new `run_plugin_prompt`
verb. The namespace (`plugin:<id>:<kind>`) is how the host keeps third-party
artifacts identifiable while routing them through first-party surfaces.

---

## What plugins *cannot* do

- **Mint a new MCP tool.** The MCP tool catalog is the closed first-party set
  (`CANONICAL_MCP_TOOLS` in `crates/wire-schema/src/parity.rs`). The parity test
  `mcp_coverage_evidence_matches_canonical_catalog` fails CI if any registry row
  — including the plugin pass-through — claims an MCP tool that isn't in that
  catalog.
- **Add a new Web route or dashboard page.** No `/v1/plugins/...` per-plugin
  endpoints exist beyond the read-only operator surface below.
- **Add a new CLI verb.** The `ogeo plugin` verbs (below) are the only
  plugin-related commands; a plugin cannot register `ogeo my-plugin-thing`.
- **Run unsandboxed or escape its capability grant** (see Load-path gates).

The single **operator-facing** API surface plugins touch is read-only:
`GET /v1/plugins` ([`apps/api/src/routes/plugins.rs`](../apps/api/src/routes/plugins.rs)),
which reports each installed plugin's runtime load status
(`loaded | skipped | load_error`). It mints no plugin-driven capability — it is a
diagnostics view of the load report, rendered identically by `ogeo plugin list`.

---

## Load-path gates — what it takes for a plugin to be active

Installing a plugin does not activate it. At `anseo serve` / worker startup the
host eagerly scans the install directory and computes a load decision per plugin
([`crates/plugin-host/src/loader.rs`](../crates/plugin-host/src/loader.rs),
`scan_and_load`). Each plugin resolves to one of three states **before** the
server accepts its first request (fail-fast, not first-request latency):

| State | Meaning |
|---|---|
| `loaded` | Passed every gate; registered for prompt runs. |
| `skipped` | Intentionally not loaded — a policy decision, not an error. |
| `load_error` | Bundle is malformed or its recorded `kind` disagrees with its manifest. Logged WARN; **serve continues** (one bad plugin never takes down startup). |

The gates, in order:

1. **Bundle integrity.** The manifest must read and parse, or the plugin is
   `load_error`.
2. **Signature gate.** A plugin recorded as `unsigned` is **skipped** unless the
   operator opts in (`LoadPolicy::allow_unsigned` / `--allow-unsigned`). An
   unverified plugin is never silently loaded into a privileged registry.
3. **Platform sandbox gate.** `analytics` plugins run in the subprocess
   seccomp-bpf / `sandbox-exec` sandbox, which is Linux/macOS only. On an
   unsupported platform (e.g. Windows) the plugin is **skipped** with an explicit
   `sandbox not supported on this platform` reason — never loaded in-process.

A freshly installed plugin requires a restart to take effect, exactly as
`ogeo plugin install` instructs.

---

## Signature & trust requirements

Signature verification happens at **install** time
([`crates/plugin-host/src/signing.rs`](../crates/plugin-host/src/signing.rs)) —
Ed25519 + TOFU, offline by construction (root keys are compile-pinned). The
chain:

1. The plugin `(id, version)` is not revoked.
2. The signing key `(namespace, keyid)` is not revoked.
3. The namespace claim is signed by a compile-pinned `OPENGEO_ROOT_PUBKEY` (or a
   root-signed rotation thereof).
4. The detached signature verifies over `SHA-256(manifest.yaml || entrypoint)` — the shipped manifest is `manifest.yaml` (the architecture's `plugin.toml` name notwithstanding; Anseo ships YAML manifests).
5. **TOFU:** first sight of a namespace pins its author key; a later key change is
   refused unless the namespace claim carries a root-signed `rotation_of`.

The load-path signature gate (above) then refuses to *activate* anything that
wasn't verified, unless `--allow-unsigned` is set.

---

## Capability sandbox limits

A plugin declares a **closed catalog** of capabilities in its manifest; the host
enforces them at call time
([`crates/plugin-host/src/capability.rs`](../crates/plugin-host/src/capability.rs)).
A call outside the grant is a **structured refusal**
(`CapabilityViolation`) plus a `plugin.capability.violation` audit event — never
a panic or a sandbox escape:

| Capability | Bound |
|---|---|
| `host:http/fetch` | Only hosts in the declared network allowlist. |
| `host:secret/read` | Only the declared secret ids. |
| `emit-event` | Only the declared event kinds. |
| `extractor-confidence-override` | Off unless declared; else the host computes confidence. |
| `analytics-window` | Requests wider than the declared max window are refused. |

Widening the capability set on upgrade is a **breaking** change: the host refuses
the upgrade unless the operator passes `--accept-new-capabilities`.

---

## Plugin id validation

A plugin id is `namespace/name`. The manifest validation pass
([`crates/plugin-manifest/src/validation.rs`](../crates/plugin-manifest/src/validation.rs))
constrains the name to a **DNS-safe** character set — lowercase `a–z`, digits,
and `-`, `.`, `_` only, max 128 chars. A `:` is **not** in that set, so an id
containing `:` (the character reserved for the `plugin:<id>:<kind>` namespace)
fails validation as an `InvalidName` error. This keeps the user-supplied id
portion from colliding with the namespace separator.

---

## The `ogeo plugin` verbs

These are the only plugin-related CLI commands; a plugin cannot add more.

| Command | Does |
|---|---|
| `ogeo plugin validate <path>` | Pure-data manifest validation (no load/verify). |
| `ogeo plugin search <query>` | Search the registry index. |
| `ogeo plugin install <ns/name[@ver]>` | Download, verify signature, install. |
| `ogeo plugin list` | List installed plugins + load status (same view as `GET /v1/plugins`). |
| `ogeo plugin remove <id>` | Remove a plugin. |
| `ogeo plugin upgrade <ns/name[@ver]>` | Upgrade (capability widening needs `--accept-new-capabilities`). |

See the [CLI manual → Plugins](./manual/cli.md#11-plugins--ogeo-plugin-phase-3)
for flags and use cases.

---

## See also

- [`crates/wire-schema/src/parity.rs`](../crates/wire-schema/src/parity.rs) — the capability registry and the `plugin_namespaced_passthrough` exception.
- [`crates/wire-schema/tests/parity_contract.rs`](../crates/wire-schema/tests/parity_contract.rs) — the CI contract that enforces it.
- [Open-core boundary](./open-core-boundary.md) — what ships MIT-OSS vs. overlay.
- [SDK spec](./sdk-spec.md) — the plugin SDK surface.
- [CLI manual](./manual/cli.md) · [MCP manual](./manual/mcp.md) · [Web manual](./manual/web.md)
