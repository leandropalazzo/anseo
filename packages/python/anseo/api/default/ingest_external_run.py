from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.ingest_run_request import IngestRunRequest
from ...models.ingest_run_response import IngestRunResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    body: IngestRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/ingest/run",
    }

    _body = body.to_dict()

    _kwargs["json"] = _body
    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[Error, IngestRunResponse]]:
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


def _build_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Response[Union[Error, IngestRunResponse]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    body: IngestRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[Error, IngestRunResponse]]:
    """Record an externally-executed prompt run, feeding the same extraction -> redaction -> envelope-
    sealed-contribution path as native runs.

     Records a prompt run executed against a provider OUTSIDE Anseo's own orchestrator (e.g. via an SDK).
    The run is persisted as a prompt_run for the project resolved from the X-Anseo-Project header and
    returns 202 with the new run_id. The optional `contribute` flag (default false) opts this run into
    the anonymous benchmark: a `contribute: true` request with no per-project KEK is rejected 403
    kek_missing; `contribute: false` proceeds regardless of KEK state. A run is redacted (same compile-
    time-safe Redactor as native runs) and envelope-sealed under the project KEK only when it set
    `contribute: true` AND the project has an active benchmark opt-in on the current terms; benchmark
    data is never silently dropped (Story 40.4).

    Args:
        x_anseo_project (Union[Unset, str]):
        body (IngestRunRequest): One externally-executed prompt run submitted for ingestion.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, IngestRunResponse]]
    """

    kwargs = _get_kwargs(
        body=body,
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: Union[AuthenticatedClient, Client],
    body: IngestRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[Error, IngestRunResponse]]:
    """Record an externally-executed prompt run, feeding the same extraction -> redaction -> envelope-
    sealed-contribution path as native runs.

     Records a prompt run executed against a provider OUTSIDE Anseo's own orchestrator (e.g. via an SDK).
    The run is persisted as a prompt_run for the project resolved from the X-Anseo-Project header and
    returns 202 with the new run_id. The optional `contribute` flag (default false) opts this run into
    the anonymous benchmark: a `contribute: true` request with no per-project KEK is rejected 403
    kek_missing; `contribute: false` proceeds regardless of KEK state. A run is redacted (same compile-
    time-safe Redactor as native runs) and envelope-sealed under the project KEK only when it set
    `contribute: true` AND the project has an active benchmark opt-in on the current terms; benchmark
    data is never silently dropped (Story 40.4).

    Args:
        x_anseo_project (Union[Unset, str]):
        body (IngestRunRequest): One externally-executed prompt run submitted for ingestion.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, IngestRunResponse]
    """

    return sync_detailed(
        client=client,
        body=body,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    body: IngestRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[Error, IngestRunResponse]]:
    """Record an externally-executed prompt run, feeding the same extraction -> redaction -> envelope-
    sealed-contribution path as native runs.

     Records a prompt run executed against a provider OUTSIDE Anseo's own orchestrator (e.g. via an SDK).
    The run is persisted as a prompt_run for the project resolved from the X-Anseo-Project header and
    returns 202 with the new run_id. The optional `contribute` flag (default false) opts this run into
    the anonymous benchmark: a `contribute: true` request with no per-project KEK is rejected 403
    kek_missing; `contribute: false` proceeds regardless of KEK state. A run is redacted (same compile-
    time-safe Redactor as native runs) and envelope-sealed under the project KEK only when it set
    `contribute: true` AND the project has an active benchmark opt-in on the current terms; benchmark
    data is never silently dropped (Story 40.4).

    Args:
        x_anseo_project (Union[Unset, str]):
        body (IngestRunRequest): One externally-executed prompt run submitted for ingestion.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, IngestRunResponse]]
    """

    kwargs = _get_kwargs(
        body=body,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: Union[AuthenticatedClient, Client],
    body: IngestRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[Error, IngestRunResponse]]:
    """Record an externally-executed prompt run, feeding the same extraction -> redaction -> envelope-
    sealed-contribution path as native runs.

     Records a prompt run executed against a provider OUTSIDE Anseo's own orchestrator (e.g. via an SDK).
    The run is persisted as a prompt_run for the project resolved from the X-Anseo-Project header and
    returns 202 with the new run_id. The optional `contribute` flag (default false) opts this run into
    the anonymous benchmark: a `contribute: true` request with no per-project KEK is rejected 403
    kek_missing; `contribute: false` proceeds regardless of KEK state. A run is redacted (same compile-
    time-safe Redactor as native runs) and envelope-sealed under the project KEK only when it set
    `contribute: true` AND the project has an active benchmark opt-in on the current terms; benchmark
    data is never silently dropped (Story 40.4).

    Args:
        x_anseo_project (Union[Unset, str]):
        body (IngestRunRequest): One externally-executed prompt run submitted for ingestion.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, IngestRunResponse]
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
