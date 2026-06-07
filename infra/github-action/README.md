# Anseo check-visibility GitHub Action

Run an Anseo visibility check inside CI; fail the build if a Brand's ranking is worse than the expected threshold.

## Usage

```yaml
- name: Check brand visibility
  uses: leandropalazzo/anseo/infra/github-action@v1
  with:
    prompt: "best vector database"
    brand: "Pinecone"
    expect-rank-lte: 3
  env:
    ANSEO_API_KEY: ${{ secrets.ANSEO_API_KEY }}
```

## Inputs

| Input             | Required | Default                       | Notes                                                                               |
| ----------------- | -------- | ----------------------------- | ----------------------------------------------------------------------------------- |
| `prompt`          | yes      | —                             | Declared Prompt slug (must exist in your `anseo.yaml`).                             |
| `brand`           | yes      | —                             | Brand name to check, as declared in `anseo.yaml`.                                   |
| `expect-rank-lte` | yes      | —                             | Maximum acceptable ranking (1 = top). Build fails if the observed rank is higher.   |
| `provider`        | no       | empty (all declared)          | Restrict to one Provider: `openai`, `anthropic`, `gemini`, `perplexity`, `grok`, `mistral`, `openrouter`. |
| `api-base`        | no       | `https://api.anseo.ai`     | Override for self-hosted deployments.                                               |

## Outputs

| Output          | Notes                                                                |
| --------------- | -------------------------------------------------------------------- |
| `observed-rank` | The observed ranking position (integer, or `null` when the brand was absent). |
| `matched-runs`  | Count of Prompt Runs evaluated for this check.                       |

## Exit codes

- `0` — within threshold.
- `1` — ranking worse than `expect-rank-lte`. Standard CI failure.
- `2` — provider returned an error (auth, rate-limit, 5xx). Distinct from a regression so consumer workflows can branch.
- `64` — missing required input (no `ANSEO_API_KEY`, missing arg).

## Step summary

The action writes a Markdown summary table to `$GITHUB_STEP_SUMMARY` (FR-44), so the PR's check-run view shows the result inline without a click-through to the run log.

## Distribution & pinning

This is a **monorepo action** — it lives at `infra/github-action` inside
[`leandropalazzo/anseo`](https://github.com/leandropalazzo/anseo), so consumers reference it by path:

```yaml
uses: leandropalazzo/anseo/infra/github-action@v1
```

Pin to a tag (`@v1` major float, or an exact `@vX.Y.Z`) and let semver protect you.

The Docker image bakes in the `anseo` CLI, which it downloads at build time from the
matching GitHub **Release asset** (`anseo-x86_64-unknown-linux-musl` /
`anseo-aarch64-unknown-linux-musl`). Those assets are produced by the release train's
`cli-binaries` job (`.github/workflows/release.yml`). The action's default
`ANSEO_VERSION` must therefore point at a tag for which that job has run — the **first
working tag is the next release cut after this action was made publishable**. Override
per-build with `--build-arg ANSEO_VERSION=<X.Y.Z>` if you need a specific CLI version.

If the asset is missing for the pinned version, the image build fails fast with an
actionable error rather than shipping a broken (404'd) binary.
