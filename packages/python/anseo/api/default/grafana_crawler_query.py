from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.grafana_crawler_query import GrafanaCrawlerQuery
from ...models.grafana_crawler_series import GrafanaCrawlerSeries
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    body: GrafanaCrawlerQuery,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/grafana/crawlers/query",
    }

    _body = body.to_dict()

    _kwargs["json"] = _body
    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[Error, list["GrafanaCrawlerSeries"]]]:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = GrafanaCrawlerSeries.from_dict(response_200_item_data)

            response_200.append(response_200_item)

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
) -> Response[Union[Error, list["GrafanaCrawlerSeries"]]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    body: GrafanaCrawlerQuery,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[Error, list["GrafanaCrawlerSeries"]]]:
    """Roadmap Epic 31 — Grafana JSON datasource query for crawler trends.

    Args:
        x_anseo_project (Union[Unset, str]):
        body (GrafanaCrawlerQuery):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, list['GrafanaCrawlerSeries']]]
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
    body: GrafanaCrawlerQuery,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[Error, list["GrafanaCrawlerSeries"]]]:
    """Roadmap Epic 31 — Grafana JSON datasource query for crawler trends.

    Args:
        x_anseo_project (Union[Unset, str]):
        body (GrafanaCrawlerQuery):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, list['GrafanaCrawlerSeries']]
    """

    return sync_detailed(
        client=client,
        body=body,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    body: GrafanaCrawlerQuery,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[Error, list["GrafanaCrawlerSeries"]]]:
    """Roadmap Epic 31 — Grafana JSON datasource query for crawler trends.

    Args:
        x_anseo_project (Union[Unset, str]):
        body (GrafanaCrawlerQuery):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, list['GrafanaCrawlerSeries']]]
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
    body: GrafanaCrawlerQuery,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[Error, list["GrafanaCrawlerSeries"]]]:
    """Roadmap Epic 31 — Grafana JSON datasource query for crawler trends.

    Args:
        x_anseo_project (Union[Unset, str]):
        body (GrafanaCrawlerQuery):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, list['GrafanaCrawlerSeries']]
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
