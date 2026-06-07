from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.recommendation import Recommendation
from ...types import UNSET, Response, Unset


def _get_kwargs(
    id: str,
    *,
    x_anseo_project: str | Unset = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/recommendations/{id}".format(
            id=quote(str(id), safe=""),
        ),
    }

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | Recommendation | None:
    if response.status_code == 200:
        response_200 = Recommendation.from_dict(response.json())

        return response_200

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if response.status_code == 404:
        response_404 = Error.from_dict(response.json())

        return response_404

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Error | Recommendation]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    x_anseo_project: str | Unset = UNSET,
) -> Response[Error | Recommendation]:
    """Story 19.6 — one recommendation + full traceability. 404 when the row is absent or owned by another
    project.

    Args:
        id (str): Recommendation ULID.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | Recommendation]
    """

    kwargs = _get_kwargs(
        id=id,
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    x_anseo_project: str | Unset = UNSET,
) -> Error | Recommendation | None:
    """Story 19.6 — one recommendation + full traceability. 404 when the row is absent or owned by another
    project.

    Args:
        id (str): Recommendation ULID.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | Recommendation
    """

    return sync_detailed(
        id=id,
        client=client,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    x_anseo_project: str | Unset = UNSET,
) -> Response[Error | Recommendation]:
    """Story 19.6 — one recommendation + full traceability. 404 when the row is absent or owned by another
    project.

    Args:
        id (str): Recommendation ULID.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | Recommendation]
    """

    kwargs = _get_kwargs(
        id=id,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    x_anseo_project: str | Unset = UNSET,
) -> Error | Recommendation | None:
    """Story 19.6 — one recommendation + full traceability. 404 when the row is absent or owned by another
    project.

    Args:
        id (str): Recommendation ULID.
        x_anseo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | Recommendation
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
