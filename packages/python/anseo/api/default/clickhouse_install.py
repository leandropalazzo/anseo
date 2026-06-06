from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.click_house_install_accepted import ClickHouseInstallAccepted
from ...models.error import Error
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    x_anseo_project: str | Unset = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/setup/clickhouse/install",
    }

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ClickHouseInstallAccepted | Error | None:
    if response.status_code == 202:
        response_202 = ClickHouseInstallAccepted.from_dict(response.json())

        return response_202

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ClickHouseInstallAccepted | Error]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    x_anseo_project: str | Unset = UNSET,
) -> Response[ClickHouseInstallAccepted | Error]:
    """Story 15.1 — kick off (MOCK) ClickHouse local-install state machine. Returns 202 with a ULID and an
    SSE stream URL. Real Docker calls land in Story 15.3.

    Args:
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ClickHouseInstallAccepted | Error]
    """

    kwargs = _get_kwargs(
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    x_anseo_project: str | Unset = UNSET,
) -> ClickHouseInstallAccepted | Error | None:
    """Story 15.1 — kick off (MOCK) ClickHouse local-install state machine. Returns 202 with a ULID and an
    SSE stream URL. Real Docker calls land in Story 15.3.

    Args:
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ClickHouseInstallAccepted | Error
    """

    return sync_detailed(
        client=client,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    x_anseo_project: str | Unset = UNSET,
) -> Response[ClickHouseInstallAccepted | Error]:
    """Story 15.1 — kick off (MOCK) ClickHouse local-install state machine. Returns 202 with a ULID and an
    SSE stream URL. Real Docker calls land in Story 15.3.

    Args:
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ClickHouseInstallAccepted | Error]
    """

    kwargs = _get_kwargs(
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    x_anseo_project: str | Unset = UNSET,
) -> ClickHouseInstallAccepted | Error | None:
    """Story 15.1 — kick off (MOCK) ClickHouse local-install state machine. Returns 202 with a ULID and an
    SSE stream URL. Real Docker calls land in Story 15.3.

    Args:
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ClickHouseInstallAccepted | Error
    """

    return (
        await asyncio_detailed(
            client=client,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
