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
| **Overlay submodule bump** | 38.18 | `opengeo-internal` repoints its submodule at the tag + green-gates | resolve |
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
its own — it runs only via `workflow_call` (from the train) or a manual
`workflow_dispatch` (operator re-publish of a single ref). There is no second
workflow that publishes images on a tag.

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

## Secrets & the public-repo path

Every cross-repo / publish leg derives a non-secret `enabled` boolean from the
**presence** of its token (never its value) and gates on that. In the public
`opengeo` repo, which holds no publish tokens, those legs skip cleanly rather
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
