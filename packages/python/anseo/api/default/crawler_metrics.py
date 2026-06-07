from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.crawler_metrics_response import CrawlerMetricsResponse
from ...models.error import Error
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    days: int | Unset = 30,
    include_unverified: bool | Unset = False,
    x_anseo_project: str | Unset = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    params: dict[str, Any] = {}

    params["days"] = days

    params["include_unverified"] = include_unverified

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/crawlers/metrics",
        "params": params,
    }

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> CrawlerMetricsResponse | Error | None:
    if response.status_code == 200:
        response_200 = CrawlerMetricsResponse.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = Error.from_dict(response.json())

        return response_400

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[CrawlerMetricsResponse | Error]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    days: int | Unset = 30,
    include_unverified: bool | Unset = False,
    x_anseo_project: str | Unset = UNSET,
) -> Response[CrawlerMetricsResponse | Error]:
    """Roadmap Epic 31 — crawler frequency, top paths, stuck/error pages, and trend. Verified-only by
    default.

    Args:
        days (int | Unset):  Default: 30.
        include_unverified (bool | Unset):  Default: False.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[CrawlerMetricsResponse | Error]
    """

    kwargs = _get_kwargs(
        days=days,
        include_unverified=include_unverified,
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    days: int | Unset = 30,
    include_unverified: bool | Unset = False,
    x_anseo_project: str | Unset = UNSET,
) -> CrawlerMetricsResponse | Error | None:
    """Roadmap Epic 31 — crawler frequency, top paths, stuck/error pages, and trend. Verified-only by
    default.

    Args:
        days (int | Unset):  Default: 30.
        include_unverified (bool | Unset):  Default: False.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        CrawlerMetricsResponse | Error
    """

    return sync_detailed(
        client=client,
        days=days,
        include_unverified=include_unverified,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    days: int | Unset = 30,
    include_unverified: bool | Unset = False,
    x_anseo_project: str | Unset = UNSET,
) -> Response[CrawlerMetricsResponse | Error]:
    """Roadmap Epic 31 — crawler frequency, top paths, stuck/error pages, and trend. Verified-only by
    default.

    Args:
        days (int | Unset):  Default: 30.
        include_unverified (bool | Unset):  Default: False.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[CrawlerMetricsResponse | Error]
    """

    kwargs = _get_kwargs(
        days=days,
        include_unverified=include_unverified,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    days: int | Unset = 30,
    include_unverified: bool | Unset = False,
    x_anseo_project: str | Unset = UNSET,
) -> CrawlerMetricsResponse | Error | None:
    """Roadmap Epic 31 — crawler frequency, top paths, stuck/error pages, and trend. Verified-only by
    default.

    Args:
        days (int | Unset):  Default: 30.
        include_unverified (bool | Unset):  Default: False.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        CrawlerMetricsResponse | Error
    """

    return (
        await asyncio_detailed(
            client=client,
            days=days,
            include_unverified=include_unverified,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
