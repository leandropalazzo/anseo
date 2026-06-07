# Anseo check-visibility GitHub Action

Run an Anseo visibility check inside CI; fail the build if a Brand's ranking is worse than the expected threshold.

## Usage

```yaml
- name: Check brand visibility
  uses: opengeo/check-visibility@v1
  with:
    prompt: "best vector database"
    brand: "Pinecone"
    expect-rank-lte: 3
  env:
    OPENGEO_API_KEY: ${{ secrets.OPENGEO_API_KEY }}
```

## Inputs

| Input             | Required | Default                       | Notes                                                                               |
| ----------------- | -------- | ----------------------------- | ----------------------------------------------------------------------------------- |
| `prompt`          | yes      | —                             | Declared Prompt slug (must exist in your `opengeo.yaml`).                           |
| `brand`           | yes      | —                             | Brand name to check, as declared in `opengeo.yaml`.                                 |
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
- `64` — missing required input (no `OPENGEO_API_KEY`, missing arg).

## Step summary

The action writes a Markdown summary table to `$GITHUB_STEP_SUMMARY` (FR-44), so the PR's check-run view shows the result inline without a click-through to the run log.

## Pinning

The action is versioned independently of the Anseo CLI; pin to a tag (`@v1`) and let semver protect you. The CLI version baked into the container is documented in the release notes for each action tag.
