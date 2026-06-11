from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.operator_verification_throughput_response_200 import (
    OperatorVerificationThroughputResponse200,
)
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    window_hours: int | Unset = 24,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["window_hours"] = window_hours

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/operator/verification/throughput",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | OperatorVerificationThroughputResponse200 | None:
    if response.status_code == 200:
        response_200 = OperatorVerificationThroughputResponse200.from_dict(
            response.json()
        )

        return response_200

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
) -> Response[Error | OperatorVerificationThroughputResponse200]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    window_hours: int | Unset = 24,
) -> Response[Error | OperatorVerificationThroughputResponse200]:
    """Story 49.0 (D1) — recent verification completions/failures over the 48.4 verification_attempts
    substrate (counts by terminal status over a look-back window). Read-only. Operator-scoped; tenant
    keys 403.

    Args:
        window_hours (int | Unset):  Default: 24.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorVerificationThroughputResponse200]
    """

    kwargs = _get_kwargs(
        window_hours=window_hours,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    window_hours: int | Unset = 24,
) -> Error | OperatorVerificationThroughputResponse200 | None:
    """Story 49.0 (D1) — recent verification completions/failures over the 48.4 verification_attempts
    substrate (counts by terminal status over a look-back window). Read-only. Operator-scoped; tenant
    keys 403.

    Args:
        window_hours (int | Unset):  Default: 24.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorVerificationThroughputResponse200
    """

    return sync_detailed(
        client=client,
        window_hours=window_hours,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    window_hours: int | Unset = 24,
) -> Response[Error | OperatorVerificationThroughputResponse200]:
    """Story 49.0 (D1) — recent verification completions/failures over the 48.4 verification_attempts
    substrate (counts by terminal status over a look-back window). Read-only. Operator-scoped; tenant
    keys 403.

    Args:
        window_hours (int | Unset):  Default: 24.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorVerificationThroughputResponse200]
    """

    kwargs = _get_kwargs(
        window_hours=window_hours,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    window_hours: int | Unset = 24,
) -> Error | OperatorVerificationThroughputResponse200 | None:
    """Story 49.0 (D1) — recent verification completions/failures over the 48.4 verification_attempts
    substrate (counts by terminal status over a look-back window). Read-only. Operator-scoped; tenant
    keys 403.

    Args:
        window_hours (int | Unset):  Default: 24.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorVerificationThroughputResponse200
    """

    return (
        await asyncio_detailed(
            client=client,
            window_hours=window_hours,
        )
    ).parsed
