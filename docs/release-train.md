# Release train — one tag fans out everywhere (Story 38.19)

Cutting a release is a single action: **push a `vX.Y.Z` tag**. From that one tag
the release-train workflow (`.github/workflows/release.yml`) fans out to every
downstream artifact and reports a single pass/fail summary. There is no
per-artifact manual step.

```bash
git tag v0.5.0
git push origin v0.5.0
# → the release-train runs; watch the run's "Release summary" job.
```

A manual re-run (or a dry run) is available via **Actions → release →
Run workflow** (`workflow_dispatch`), which takes a `ref` and a `dry_run` flag.

## What it triggers (the fan-out set)

| Leg | Story | What it produces | Depends on |
| --- | ----- | ---------------- | ---------- |
| **GHCR images** | 38.15 | multi-arch `api`/`worker`/`web` images pinned to `X.Y.Z` (+ `X.Y`, `X`) with SLSA provenance | resolve |
| **Standalone compose snapshot** | 38.16 | pinned `compose.yml` published to `https://anseo.ai/compose/<X.Y.Z>.yml` | images |
| **SDK publish** | 40.x | `@anseo/observe` → npm, `anseo-observe` → PyPI | resolve |
| **Plugin sign/publish** | 41.4 | signed first-party plugin bundle via `plugin-sign.yml` | resolve |
| **Overlay submodule bump** | 38.18 | `anseo-internal` repoints its submodule at the tag + green-gates | resolve |
| **Docs/marketing site** | 38.21 | site rebuilt against the tag | resolve |
| **GitHub Release** | 38.19 | the user-facing Release object with notes + the compose snapshot attached | images, compose snapshot |
| **Release summary** | 38.19 | one job; reports every leg and **fails the train if any leg failed** | all legs (`always()`) |

The **artifact spine** is `images → compose-snapshot → github-release`: the
snapshot pins the images it references, and the Release attaches the snapshot, so
each only runs once its inputs exist. The cross-repo / publish legs
(`sdk-publish`, `plugin-sign`, `overlay-bump`, `docs-site`) are independent and
run in parallel off `resolve`. There are no dependency cycles.

## Version normalization (the leading-`v` rule)

The git tag carries a leading `v` (`v0.5.0`); every published artifact uses the
**bare `X.Y.Z`** form (`0.5.0`):

- `docker/metadata-action` `type=semver,pattern={{version}}` strips the `v`, so
  the images are tagged `0.5.0`.
- The standalone bundle's `ANSEO_VERSION` is `X.Y.Z` (no `v`), and the pinned
  snapshot bakes `0.5.0` in.

The train's `resolve` job derives the bare version once and every leg consumes it,
so the images, the snapshot, and `https://anseo.ai/compose/0.5.0.yml` always
agree.

## No double-publish

The image build/publish logic lives in **exactly one place**: the reusable
workflow `.github/workflows/release-images.yml` (the refactored Story 38.15
logic). The release-train is its **only tag-triggered caller** (`uses:`), so a
tag publishes the images exactly once. `release-images.yml` has no tag trigger of
its own — it runs only via `workflow_call` from the guarded train. There is no
second workflow that publishes images on a tag.

After a real tag publish, verify the GHCR artifact evidence from a clean Docker
config:

```bash
scripts/verify-release-images.sh 0.5.0
```

The verifier checks unauthenticated pullability, `linux/amd64` and `linux/arm64`
manifest entries, and basic image-history secret/default guards for `api`,
`worker`, and `web`.

## It can never block PRs

The release-train and `release-images.yml` are triggered **only** by a tag push
or `workflow_dispatch`. Neither has a `pull_request` or `push: branches` trigger,
and none of their jobs are required PR checks. The heavy build/publish jobs are
gated on the tag/dispatch event itself, so an ordinary PR never spins up a
release job. (This is the structural guard against the prior regression where a
release-only job was wired as a PR-blocking CI check.)

## Partial failures are never silent (AC2)

The `release-summary` job runs with `if: always()` and inspects every leg's
`result`. If any leg ended in `failure` or `cancelled`, the summary job **fails**
and the release run is red, with the failing legs named in the job summary table.
A leg that was `skipped` — because its token isn't configured (public repo) or
because of `dry_run` — is acceptable and does **not** fail the train.

## Supply-chain hardening: protected `release` environment + ref restriction

The publish/signing tokens (`NPM_TOKEN`, `PYPI_API_TOKEN`, the plugin-sign /
registry / overlay / docs-site dispatch tokens, and the real plugin signing key)
are protected by **two independent layers**. Either one alone stops an attacker
who can merely *dispatch* the workflow from exfiltrating a token or publishing an
unreviewed artifact; together they fail closed.

### 1. Protected `release` environment (maintainer approval)

Every job that can expose a publish/signing secret declares
`environment: release`:

- `release.yml` → `compose-snapshot`, `sdk-publish`, `plugin-sign`,
  `overlay-bump`, `docs-site`
- `plugin-sign.yml` → `sign` (the real root key + registry token)

GitHub does **not** materialize an environment's secrets until the deployment is
approved, so a maintainer must approve the `release` deployment before any of
those runners start — even on a tag push or a `workflow_dispatch`. Jobs that need
no publish secret (`resolve`, `images` and `github-release` use only the built-in
`GITHUB_TOKEN`, `release-summary`) are **not** in the environment and run without
approval, so the image build and the release object are never blocked on
approval-less paths.

> **You must configure the `release` environment** (Settings → Environments →
> `release`) with **required reviewers**, and ideally a **deployment-protection
> rule** restricting deployments to `v*` tags / the default branch. Without
> required reviewers the gate is inert. The repo that holds a given secret is the
> repo where the environment protection matters (e.g. the real signing key lives
> in `anseo-internal`, so configure reviewers there). In the public `anseo`
> repo there are no publish secrets, so its `release` environment can be left
> without reviewers and the ephemeral-key plugin self-check still runs green.

### 2. Ref restriction (default-branch-reachable release tags only)

Before any secret-bearing leg runs, the `resolve` job proves the ref is a **real
release tag reachable from the default branch** and emits `publish_ok`:

- on **push**: `github.ref` must be `refs/tags/vX.Y.Z` (strict semver, no
  prerelease/build suffix);
- on **workflow_dispatch**: the supplied `ref` must be a strict `vX.Y.Z` tag that
  **exists** as a git tag **and** whose commit is an **ancestor of
  `origin/<default branch>`** (`git merge-base --is-ancestor`).

Anything else — an arbitrary branch/SHA, a non-semver tag, a tag that doesn't
exist, or a tag living only on an unmerged branch — yields `publish_ok=false`,
and every publish leg is `if:`-skipped (**fail closed**: no token is ever
exposed). This blocks the attack where a dispatch points at an unreviewed
branch/SHA and npm/Python lifecycle scripts or a build backend run with the
publish tokens in scope. Dry-run and the build-only paths (`images`,
`compose-snapshot` rendering, `github-release`) are unaffected.

## Secrets & the public-repo path

Every cross-repo / publish leg derives a non-secret `enabled` boolean from the
**presence** of its token (never its value) and gates on that. In the public
`anseo` repo, which holds no publish tokens, those legs skip cleanly rather
than failing; the images + compose snapshot (which need only `GITHUB_TOKEN`)
still build, and the snapshot is retained as a run artifact.

| Secret / var | Used by | If absent |
| ------------ | ------- | --------- |
| `GITHUB_TOKEN` (built-in) | images, GitHub Release | always present |
| `NPM_TOKEN` | npm publish | npm leg skipped |
| `PYPI_API_TOKEN` | PyPI publish | PyPI leg skipped |
| `DOCS_SITE_DISPATCH_TOKEN` + `vars.DOCS_SITE_REPO` | compose snapshot upload, site rebuild | snapshot kept as run artifact only; site rebuild skipped |
| `OVERLAY_DISPATCH_TOKEN` + `vars.OVERLAY_REPO` | overlay submodule bump | overlay leg skipped |
| `PLUGIN_SIGN_DISPATCH_TOKEN` + `vars.FIRST_PARTY_PLUGIN_DIR` + `vars.FIRST_PARTY_PLUGIN_NAMESPACE` | plugin sign dispatch | plugin leg skipped |

## Helper script

`scripts/make-compose-snapshot.sh <X.Y.Z> [out-dir]` renders the pinned
standalone snapshot from `infra/standalone/compose.yml` (baking `ANSEO_VERSION`
to the release version). The train calls it in the `compose-snapshot` leg; you
can also run it locally to inspect the artifact a release would publish.
