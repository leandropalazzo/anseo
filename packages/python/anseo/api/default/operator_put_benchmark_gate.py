from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.operator_benchmark_gate import OperatorBenchmarkGate
from ...models.operator_put_benchmark_gate_body import OperatorPutBenchmarkGateBody
from ...types import Response


def _get_kwargs(
    *,
    body: OperatorPutBenchmarkGateBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/v1/operator/config/benchmark-gate",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | OperatorBenchmarkGate | None:
    if response.status_code == 200:
        response_200 = OperatorBenchmarkGate.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = Error.from_dict(response.json())

        return response_400

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if response.status_code == 403:
        response_403 = Error.from_dict(response.json())

        return response_403

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Error | OperatorBenchmarkGate]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: OperatorPutBenchmarkGateBody,
) -> Response[Error | OperatorBenchmarkGate]:
    """Story 49.0 (D2) — operator-admin write of the terms-finalize gate. The source of truth lives in OSS
    (crates/storage), not anseo_admin; a subsequent GET reflects the write. Operator-scoped; tenant keys
    403.

    Args:
        body (OperatorPutBenchmarkGateBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorBenchmarkGate]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    body: OperatorPutBenchmarkGateBody,
) -> Error | OperatorBenchmarkGate | None:
    """Story 49.0 (D2) — operator-admin write of the terms-finalize gate. The source of truth lives in OSS
    (crates/storage), not anseo_admin; a subsequent GET reflects the write. Operator-scoped; tenant keys
    403.

    Args:
        body (OperatorPutBenchmarkGateBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorBenchmarkGate
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: OperatorPutBenchmarkGateBody,
) -> Response[Error | OperatorBenchmarkGate]:
    """Story 49.0 (D2) — operator-admin write of the terms-finalize gate. The source of truth lives in OSS
    (crates/storage), not anseo_admin; a subsequent GET reflects the write. Operator-scoped; tenant keys
    403.

    Args:
        body (OperatorPutBenchmarkGateBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorBenchmarkGate]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: OperatorPutBenchmarkGateBody,
) -> Error | OperatorBenchmarkGate | None:
    """Story 49.0 (D2) — operator-admin write of the terms-finalize gate. The source of truth lives in OSS
    (crates/storage), not anseo_admin; a subsequent GET reflects the write. Operator-scoped; tenant keys
    403.

    Args:
        body (OperatorPutBenchmarkGateBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorBenchmarkGate
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
