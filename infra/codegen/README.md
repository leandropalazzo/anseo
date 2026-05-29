# OpenGEO SDK codegen (Story 12.3)

Substrate for the Phase 2 TypeScript + Python SDK generation pipeline.
Both SDKs are produced from the canonical
`crates/wire-schema/openapi.json` artifact — drift between any SDK and
that artifact is a CI failure.

## Layout

```
infra/codegen/
├── README.md             # this file
├── Makefile              # `make sdks` regenerates both languages
├── orval.config.cjs      # TS client config (orval)
├── openapi-python.yaml   # Python client config (openapi-python-client)
└── tests/
    └── drift.sh          # CI gate: regenerate, byte-compare to commit
```

```
packages/typescript/      # published as @opengeo/sdk
packages/python/          # published as opengeo on PyPI
```

## Generator versions

Pinned in the Makefile so any contributor regenerating with the same
version produces byte-equal output. Bumping a generator version goes
through a deliberate PR that includes the regenerated artifacts.

## Regenerating locally

```bash
make -C infra/codegen sdks
```

…regenerates both SDKs in place. Commit the diff alongside any change
that touches `crates/wire-schema/openapi.json` so CI's drift check
stays green.

## CI drift check

```bash
bash infra/codegen/tests/drift.sh
```

Regenerates both SDKs into a tmp dir and `diff -q`s against the
committed `packages/*/`. Non-zero exit → drift → block merge.

## Status

Both generators are wired and the SDKs at `packages/typescript/` and
`packages/python/opengeo/` are now populated from the canonical spec.
`make sdks` regenerates them; `make drift` is the CI gate. Run a quick
local install with `uvx` if you don't want a system-wide install:

```bash
# Quick run without persistent install (uvx + npx)
npx -y orval@7.1.0 --config infra/codegen/orval.config.cjs
uvx openapi-python-client@0.24.0 generate \
  --path crates/wire-schema/openapi.json \
  --config infra/codegen/openapi-python.yaml \
  --meta none --output-path packages/python --overwrite
```
