# anseo (Python)

Python client for the Anseo REST API. Auto-generated from
`crates/wire-schema/openapi.json` by
`openapi-python-client@0.24.0` (everything under `anseo/api/`,
`anseo/models/`, `anseo/client.py`, `anseo/types.py`, and
`anseo/errors.py`). `anseo/auth.py` is hand-written and survives
regeneration.

## Install

```bash
pip install anseo
```

## Usage

```python
from anseo.auth import AnseoClient
from anseo.api.default import list_runs, create_prompt_run
from anseo.models import CreatePromptRunRequest, CreatePromptRunRequestProvider

client = AnseoClient(
    base_url="http://127.0.0.1:8080",
    api_key=os.environ["ANSEO_API_KEY"],
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

`AnseoClient` is a thin factory over `AuthenticatedClient` that pins
the `X-Anseo-API-Key` header and an empty token prefix — the
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
