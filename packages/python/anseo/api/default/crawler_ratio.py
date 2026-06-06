from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.crawl_refer_report import CrawlReferReport
from ...models.error import Error
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    days: Union[Unset, int] = 30,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    params: dict[str, Any] = {}

    params["days"] = days

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/crawlers/ratio",
        "params": params,
    }

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[CrawlReferReport, Error]]:
    if response.status_code == 200:
        response_200 = CrawlReferReport.from_dict(response.json())

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
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Response[Union[CrawlReferReport, Error]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    days: Union[Unset, int] = 30,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[CrawlReferReport, Error]]:
    """Roadmap Epic 33 — crawl-to-refer ratio by verified bot, degraded to crawls_only until referral
    attribution exists.

    Args:
        days (Union[Unset, int]):  Default: 30.
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[CrawlReferReport, Error]]
    """

    kwargs = _get_kwargs(
        days=days,
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: Union[AuthenticatedClient, Client],
    days: Union[Unset, int] = 30,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[CrawlReferReport, Error]]:
    """Roadmap Epic 33 — crawl-to-refer ratio by verified bot, degraded to crawls_only until referral
    attribution exists.

    Args:
        days (Union[Unset, int]):  Default: 30.
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[CrawlReferReport, Error]
    """

    return sync_detailed(
        client=client,
        days=days,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    days: Union[Unset, int] = 30,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[CrawlReferReport, Error]]:
    """Roadmap Epic 33 — crawl-to-refer ratio by verified bot, degraded to crawls_only until referral
    attribution exists.

    Args:
        days (Union[Unset, int]):  Default: 30.
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[CrawlReferReport, Error]]
    """

    kwargs = _get_kwargs(
        days=days,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: Union[AuthenticatedClient, Client],
    days: Union[Unset, int] = 30,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[CrawlReferReport, Error]]:
    """Roadmap Epic 33 — crawl-to-refer ratio by verified bot, degraded to crawls_only until referral
    attribution exists.

    Args:
        days (Union[Unset, int]):  Default: 30.
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[CrawlReferReport, Error]
    """

    return (
        await asyncio_detailed(
            client=client,
            days=days,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
