from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.recommendation_list_response import RecommendationListResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    limit: int | Unset = 50,
    cursor: str | Unset = UNSET,
    x_anseo_project: str | Unset = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    params: dict[str, Any] = {}

    params["limit"] = limit

    params["cursor"] = cursor

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/recommendations",
        "params": params,
    }

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | RecommendationListResponse | None:
    if response.status_code == 200:
        response_200 = RecommendationListResponse.from_dict(response.json())

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
) -> Response[Error | RecommendationListResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = 50,
    cursor: str | Unset = UNSET,
    x_anseo_project: str | Unset = UNSET,
) -> Response[Error | RecommendationListResponse]:
    """Story 19.6 — cursor-paginated active recommendations for the project, newest first.

    Args:
        limit (int | Unset):  Default: 50.
        cursor (str | Unset): Opaque page cursor from a previous response's next_cursor.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | RecommendationListResponse]
    """

    kwargs = _get_kwargs(
        limit=limit,
        cursor=cursor,
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = 50,
    cursor: str | Unset = UNSET,
    x_anseo_project: str | Unset = UNSET,
) -> Error | RecommendationListResponse | None:
    """Story 19.6 — cursor-paginated active recommendations for the project, newest first.

    Args:
        limit (int | Unset):  Default: 50.
        cursor (str | Unset): Opaque page cursor from a previous response's next_cursor.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | RecommendationListResponse
    """

    return sync_detailed(
        client=client,
        limit=limit,
        cursor=cursor,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = 50,
    cursor: str | Unset = UNSET,
    x_anseo_project: str | Unset = UNSET,
) -> Response[Error | RecommendationListResponse]:
    """Story 19.6 — cursor-paginated active recommendations for the project, newest first.

    Args:
        limit (int | Unset):  Default: 50.
        cursor (str | Unset): Opaque page cursor from a previous response's next_cursor.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | RecommendationListResponse]
    """

    kwargs = _get_kwargs(
        limit=limit,
        cursor=cursor,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = 50,
    cursor: str | Unset = UNSET,
    x_anseo_project: str | Unset = UNSET,
) -> Error | RecommendationListResponse | None:
    """Story 19.6 — cursor-paginated active recommendations for the project, newest first.

    Args:
        limit (int | Unset):  Default: 50.
        cursor (str | Unset): Opaque page cursor from a previous response's next_cursor.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | RecommendationListResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            limit=limit,
            cursor=cursor,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
