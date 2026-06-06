# Plugin Signing & the Anseo Trust Model

Anseo plugins are signed with **Ed25519** and trusted via **TOFU** (trust on
first use). This page explains the trust model, how a first-party plugin is
signed, and what `[UNSIGNED PLUGIN]` means when you install one.

> Story 41.4 operationalizes the verification machinery that landed in Epic 17
> (`crates/plugin-host/src/signing.rs`). The verifier is the same code the
> worker and `anseo plugin install` run; the signing tools below produce
> artifacts that verify under that exact code, so a signature we emit can never
> drift from what the install path checks.

## The trust model

Every signed plugin in the registry ships two pieces of cryptographic state:

| File              | What it is                                                                 |
| ----------------- | ------------------------------------------------------------------------- |
| `signature.bin`   | 64-byte detached Ed25519 signature by the **author key** over `SHA-256(manifest.yaml ‖ entrypoint.wasm)`. |
| `claim.toml`      | A **namespace claim** (`namespace`, `keyid`, `author_pubkey`) plus a 64-byte Ed25519 signature **by the project root key** over the claim's canonical bytes. |

At install time `verify_signed_plugin` runs this chain (architecture phase-3
§5.4):

1. The plugin `(id, version)` is not in the registry revocation list.
2. The signing key `(namespace, keyid)` is not revoked.
3. The **namespace claim is signed by a compile-pinned root key**
   (`ANSEO_ROOT_PUBKEY`). This is the root attestation that says "this author
   key owns this namespace".
4. The plugin's `signature.bin` verifies against the now-root-attested author
   key over the bundle digest.
5. **TOFU:** the author key is pinned in `trusted_keys.toml` on first install;
   a later install with a *different* key is refused unless the claim carries a
   root-signed `rotation_of` of the pinned key.

The root public key is **compile-pinned**: builds set `ANSEO_ROOT_PUBKEY` to a
comma-separated list of 64-char hex Ed25519 public keys (multiple supported for
future rotation). The deprecated name `OPENGEO_ROOT_PUBKEY` is still read for
back-compat.

## First-party vs community plugins

A plugin is **first-party** when its manifest declares `publisher: anseo.ai`.

- **First-party** plugins are **signature-required**. A missing or invalid
  signature is a hard error at install time — there is **no** `--allow-unsigned`
  escape hatch for them.
- **Community** plugins (any other / empty `publisher`) may be installed
  unsigned via `--allow-unsigned`. Install proceeds with a warning:

  ```
  [UNSIGNED PLUGIN] <id>: installing without a verified signature
  (--allow-unsigned). Anseo cannot attest this plugin's authenticity.
  ```

  The install is recorded with `signature_status = unsigned` in the audit row.

## Signing a plugin (operator / CI)

The signing producers ship in the `anseo` CLI behind the
`anseo-plugin-host/signing-tools` feature (enabled by default for the CLI).

### 1. Generate the root keypair (one time, offline)

```sh
anseo plugin keygen --out /secure/anseo-root.key
# prints:
#   public  (pin as ANSEO_ROOT_PUBKEY): <64-char hex>
#   secret  written to /secure/anseo-root.key (DO NOT COMMIT)
```

- **Public key** → pin it as the `ANSEO_ROOT_PUBKEY` build env so the verifier
  trusts it (`crates/plugin-host/src/signing.rs::pinned_root_pubkeys`).
- **Secret seed** → store it as the **`ANSEO_PLUGIN_SIGNING_KEY`** GitHub
  Actions secret in the **private `opengeo-internal`** repo. **Never** commit
  it; the public `opengeo` repo must not contain the private key (ADR-007).

### 2. Sign a bundle

```sh
export ANSEO_PLUGIN_SIGNING_KEY=<hex secret seed>   # from CI secret
anseo plugin sign plugins/anseo.core-extractor/1.0.0 \
  --namespace anseo --keyid root-2026
# writes signature.bin + claim.toml next to manifest.yaml + entrypoint.wasm
```

`anseo plugin sign` reads the root secret from `ANSEO_PLUGIN_SIGNING_KEY` (or
`--key-file`), signs the bundle digest and the namespace claim, and writes the
two artifacts the registry serves.

## CI pipeline

`.github/workflows/plugin-sign.yml` operationalizes signing:

- **Triggers:** `workflow_dispatch` (manual, with version-dir + namespace
  inputs) and `repository_dispatch` (`event_type: plugin-sign`) fired from the
  `plugin-registry` repo on a push to `main`. It is **not** a `pull_request` /
  `push` job, so it never gates ordinary PRs and is not a required check.
- **Production** (in `opengeo-internal`, where the `ANSEO_PLUGIN_SIGNING_KEY`
  secret lives): signs the requested bundle with the real root key and commits
  `signature.bin` + `claim.toml` back to the registry.
- **Public self-check** (in `opengeo`, no secret): runs the keygen→sign→verify
  round-trip with an **ephemeral** key to keep the pipeline green without ever
  exposing a real key.

### Open-core boundary (ADR-007)

| Lives in `opengeo` (public)            | Lives in `opengeo-internal` (private) |
| -------------------------------------- | ------------------------------------- |
| Verification code (`plugin-host`)      | The real root **private key** secret  |
| Signing **tools** (`anseo plugin sign`)| The production `plugin-sign` job that uses it |
| The pinned root **public** key         |                                       |

This separation is the correct boundary: anyone can verify, only the
maintainer (via the private CI secret) can produce a root-signed claim.

## Key rotation & revocation

Rotation (`rotation_of` in a claim) and revocation (`keys/revoked.toml`) are
*verified* today but the **producing** side is a future story. The signing
tools intentionally do not emit rotation claims yet — see the
`TODO(key-rotation)` markers in `signing.rs` and `plugin_sign.rs`.
