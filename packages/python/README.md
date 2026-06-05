# opengeo (Python)

Python client for the Anseo REST API. Auto-generated from
`crates/wire-schema/openapi.json` by
`openapi-python-client@0.24.0` (everything under `opengeo/api/`,
`opengeo/models/`, `opengeo/client.py`, `opengeo/types.py`, and
`opengeo/errors.py`). `opengeo/auth.py` is hand-written and survives
regeneration.

## Install

```bash
pip install opengeo
```

## Usage

```python
from opengeo.auth import OpenGeoClient
from opengeo.api.default import list_runs, create_prompt_run
from opengeo.models import CreatePromptRunRequest, CreatePromptRunRequestProvider

client = OpenGeoClient(
    base_url="http://127.0.0.1:8080",
    api_key=os.environ["OPENGEO_API_KEY"],
)

# List recent runs.
runs = list_runs.sync(client=client, limit=25)

# Trigger a one-shot mock run.
created = create_prompt_run.sync(
    client=client,
    body=CreatePromptRunRequest(
        prompt_name="vector-db",
        provider=CreatePromptRunRequestProvider.MOCK,
    ),
)
```

`OpenGeoClient` is a thin factory over `AuthenticatedClient` that pins
the `X-OpenGEO-API-Key` header and an empty token prefix — the
architecture-mandated auth shape (§5.1). Any other keyword
(`timeout`, `verify_ssl`, `httpx_args`, …) is forwarded to
`AuthenticatedClient`.

## Regenerating

After any change to `crates/wire-schema/openapi.json`:

```bash
make -C infra/codegen py
```

The CI drift gate at `infra/codegen/tests/drift.sh` blocks merges if
this package is out of sync with the canonical spec. The generator's
`--overwrite` flag only rewrites the files it emits, so `auth.py` and
this README are preserved across regenerations.
