from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.error import Error
from ...models.ingest_run_request import IngestRunRequest
from ...models.ingest_run_response import IngestRunResponse
from ...types import UNSET, Unset
from typing import cast



def _get_kwargs(
    *,
    body: IngestRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_open_geo_project, Unset):
        headers["X-OpenGEO-Project"] = x_open_geo_project



    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/ingest/run",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Error | IngestRunResponse | None:
    if response.status_code == 202:
        response_202 = IngestRunResponse.from_dict(response.json())



        return response_202

    if response.status_code == 400:
        response_400 = Error.from_dict(response.json())



        return response_400

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())



        return response_401

    if response.status_code == 403:
        response_403 = Error.from_dict(response.json())



        return response_403

    if response.status_code == 404:
        response_404 = Error.from_dict(response.json())



        return response_404

    if response.status_code == 422:
        response_422 = Error.from_dict(response.json())



        return response_422

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Error | IngestRunResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: IngestRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[Error | IngestRunResponse]:
    """ Record an externally-executed prompt run, feeding the same extraction -> redaction -> envelope-
    sealed-contribution path as native runs.

     Records a prompt run executed against a provider OUTSIDE OpenGEO's own orchestrator (e.g. via an
    SDK). The run is persisted as a prompt_run for the project resolved from the X-OpenGEO-Project
    header and returns 202 with the new run_id. The optional `contribute` flag (default false) opts this
    run into the anonymous benchmark: a `contribute: true` request with no per-project KEK is rejected
    403 kek_missing; `contribute: false` proceeds regardless of KEK state. A run is redacted and
    envelope-sealed under the project KEK only when it set `contribute: true` AND the project has an
    active benchmark opt-in on the current terms; benchmark data is never silently dropped.

    Args:
        x_open_geo_project (str | Unset):
        body (IngestRunRequest): One externally-executed prompt run submitted for ingestion.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | IngestRunResponse]
     """


    kwargs = _get_kwargs(
        body=body,
x_open_geo_project=x_open_geo_project,

    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)

def sync(
    *,
    client: AuthenticatedClient | Client,
    body: IngestRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> Error | IngestRunResponse | None:
    """ Record an externally-executed prompt run, feeding the same extraction -> redaction -> envelope-
    sealed-contribution path as native runs.

     Records a prompt run executed against a provider OUTSIDE OpenGEO's own orchestrator (e.g. via an
    SDK). The run is persisted as a prompt_run for the project resolved from the X-OpenGEO-Project
    header and returns 202 with the new run_id. The optional `contribute` flag (default false) opts this
    run into the anonymous benchmark: a `contribute: true` request with no per-project KEK is rejected
    403 kek_missing; `contribute: false` proceeds regardless of KEK state. A run is redacted and
    envelope-sealed under the project KEK only when it set `contribute: true` AND the project has an
    active benchmark opt-in on the current terms; benchmark data is never silently dropped.

    Args:
        x_open_geo_project (str | Unset):
        body (IngestRunRequest): One externally-executed prompt run submitted for ingestion.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | IngestRunResponse
     """


    return sync_detailed(
        client=client,
body=body,
x_open_geo_project=x_open_geo_project,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: IngestRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[Error | IngestRunResponse]:
    """ Record an externally-executed prompt run, feeding the same extraction -> redaction -> envelope-
    sealed-contribution path as native runs.

     Records a prompt run executed against a provider OUTSIDE OpenGEO's own orchestrator (e.g. via an
    SDK). The run is persisted as a prompt_run for the project resolved from the X-OpenGEO-Project
    header and returns 202 with the new run_id. The optional `contribute` flag (default false) opts this
    run into the anonymous benchmark: a `contribute: true` request with no per-project KEK is rejected
    403 kek_missing; `contribute: false` proceeds regardless of KEK state. A run is redacted and
    envelope-sealed under the project KEK only when it set `contribute: true` AND the project has an
    active benchmark opt-in on the current terms; benchmark data is never silently dropped.

    Args:
        x_open_geo_project (str | Unset):
        body (IngestRunRequest): One externally-executed prompt run submitted for ingestion.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | IngestRunResponse]
     """


    kwargs = _get_kwargs(
        body=body,
x_open_geo_project=x_open_geo_project,

    )

    response = await client.get_async_httpx_client().request(
        **kwargs
    )

    return _build_response(client=client, response=response)

async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: IngestRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> Error | IngestRunResponse | None:
    """ Record an externally-executed prompt run, feeding the same extraction -> redaction -> envelope-
    sealed-contribution path as native runs.

     Records a prompt run executed against a provider OUTSIDE OpenGEO's own orchestrator (e.g. via an
    SDK). The run is persisted as a prompt_run for the project resolved from the X-OpenGEO-Project
    header and returns 202 with the new run_id. The optional `contribute` flag (default false) opts this
    run into the anonymous benchmark: a `contribute: true` request with no per-project KEK is rejected
    403 kek_missing; `contribute: false` proceeds regardless of KEK state. A run is redacted and
    envelope-sealed under the project KEK only when it set `contribute: true` AND the project has an
    active benchmark opt-in on the current terms; benchmark data is never silently dropped.

    Args:
        x_open_geo_project (str | Unset):
        body (IngestRunRequest): One externally-executed prompt run submitted for ingestion.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | IngestRunResponse
     """


    return (await asyncio_detailed(
        client=client,
body=body,
x_open_geo_project=x_open_geo_project,

    )).parsed
