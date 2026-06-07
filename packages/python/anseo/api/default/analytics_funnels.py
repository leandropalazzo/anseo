from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.analytics_funnels_period import AnalyticsFunnelsPeriod
from ...models.analytics_funnels_response_200 import AnalyticsFunnelsResponse200
from ...models.error import Error
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    period: AnalyticsFunnelsPeriod | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    json_period: str | Unset = UNSET
    if not isinstance(period, Unset):
        json_period = period.value

    params["period"] = json_period

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/analytics/funnels",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AnalyticsFunnelsResponse200 | Error | None:
    if response.status_code == 200:
        response_200 = AnalyticsFunnelsResponse200.from_dict(response.json())

        return response_200

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AnalyticsFunnelsResponse200 | Error]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    period: AnalyticsFunnelsPeriod | Unset = UNSET,
) -> Response[AnalyticsFunnelsResponse200 | Error]:
    """Story 47.4 — operator analytics: contribute funnel step counts + per-step drop-off (start → step →
    complete), verify funnel start/complete/fail counts by method (dns | email) with success rate, and
    daily badge-embed serves (last 30 d). Read entirely from the aggregate site-event rollups (privacy-
    safe by construction). Operator-scoped; not gated by X-Anseo-Project. No MCP parity (operator-
    internal, not agent-facing).

    Args:
        period (AnalyticsFunnelsPeriod | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AnalyticsFunnelsResponse200 | Error]
    """

    kwargs = _get_kwargs(
        period=period,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    period: AnalyticsFunnelsPeriod | Unset = UNSET,
) -> AnalyticsFunnelsResponse200 | Error | None:
    """Story 47.4 — operator analytics: contribute funnel step counts + per-step drop-off (start → step →
    complete), verify funnel start/complete/fail counts by method (dns | email) with success rate, and
    daily badge-embed serves (last 30 d). Read entirely from the aggregate site-event rollups (privacy-
    safe by construction). Operator-scoped; not gated by X-Anseo-Project. No MCP parity (operator-
    internal, not agent-facing).

    Args:
        period (AnalyticsFunnelsPeriod | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AnalyticsFunnelsResponse200 | Error
    """

    return sync_detailed(
        client=client,
        period=period,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    period: AnalyticsFunnelsPeriod | Unset = UNSET,
) -> Response[AnalyticsFunnelsResponse200 | Error]:
    """Story 47.4 — operator analytics: contribute funnel step counts + per-step drop-off (start → step →
    complete), verify funnel start/complete/fail counts by method (dns | email) with success rate, and
    daily badge-embed serves (last 30 d). Read entirely from the aggregate site-event rollups (privacy-
    safe by construction). Operator-scoped; not gated by X-Anseo-Project. No MCP parity (operator-
    internal, not agent-facing).

    Args:
        period (AnalyticsFunnelsPeriod | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AnalyticsFunnelsResponse200 | Error]
    """

    kwargs = _get_kwargs(
        period=period,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    period: AnalyticsFunnelsPeriod | Unset = UNSET,
) -> AnalyticsFunnelsResponse200 | Error | None:
    """Story 47.4 — operator analytics: contribute funnel step counts + per-step drop-off (start → step →
    complete), verify funnel start/complete/fail counts by method (dns | email) with success rate, and
    daily badge-embed serves (last 30 d). Read entirely from the aggregate site-event rollups (privacy-
    safe by construction). Operator-scoped; not gated by X-Anseo-Project. No MCP parity (operator-
    internal, not agent-facing).

    Args:
        period (AnalyticsFunnelsPeriod | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AnalyticsFunnelsResponse200 | Error
    """

    return (
        await asyncio_detailed(
            client=client,
            period=period,
        )
    ).parsed
