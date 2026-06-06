from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.run_list_response import RunListResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    limit: Union[Unset, int] = UNSET,
    offset: Union[Unset, int] = UNSET,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    params: dict[str, Any] = {}

    params["limit"] = limit

    params["offset"] = offset

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/runs",
        "params": params,
    }

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[Error, RunListResponse]]:
    if response.status_code == 200:
        response_200 = RunListResponse.from_dict(response.json())

        return response_200
    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401
    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Response[Union[Error, RunListResponse]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    limit: Union[Unset, int] = UNSET,
    offset: Union[Unset, int] = UNSET,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[Error, RunListResponse]]:
    """List recent Prompt Runs

    Args:
        limit (Union[Unset, int]):
        offset (Union[Unset, int]):
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, RunListResponse]]
    """

    kwargs = _get_kwargs(
        limit=limit,
        offset=offset,
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: Union[AuthenticatedClient, Client],
    limit: Union[Unset, int] = UNSET,
    offset: Union[Unset, int] = UNSET,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[Error, RunListResponse]]:
    """List recent Prompt Runs

    Args:
        limit (Union[Unset, int]):
        offset (Union[Unset, int]):
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, RunListResponse]
    """

    return sync_detailed(
        client=client,
        limit=limit,
        offset=offset,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    limit: Union[Unset, int] = UNSET,
    offset: Union[Unset, int] = UNSET,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[Error, RunListResponse]]:
    """List recent Prompt Runs

    Args:
        limit (Union[Unset, int]):
        offset (Union[Unset, int]):
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, RunListResponse]]
    """

    kwargs = _get_kwargs(
        limit=limit,
        offset=offset,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: Union[AuthenticatedClient, Client],
    limit: Union[Unset, int] = UNSET,
    offset: Union[Unset, int] = UNSET,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[Error, RunListResponse]]:
    """List recent Prompt Runs

    Args:
        limit (Union[Unset, int]):
        offset (Union[Unset, int]):
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, RunListResponse]
    """

    return (
        await asyncio_detailed(
            client=client,
            limit=limit,
            offset=offset,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
