from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.error import Error
from ...types import UNSET, Unset
from typing import cast
from uuid import UUID



def _get_kwargs(
    project_id: UUID,
    *,
    x_open_geo_project: str | Unset = UNSET,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_open_geo_project, Unset):
        headers["X-OpenGEO-Project"] = x_open_geo_project



    

    

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/projects/{project_id}/events".format(project_id=quote(str(project_id), safe=""),),
    }


    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Any | Error | None:
    if response.status_code == 200:
        response_200 = cast(Any, None)
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


def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Any | Error]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    project_id: UUID,
    *,
    client: AuthenticatedClient | Client,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[Any | Error]:
    """ Server-Sent Events stream of ARCH-17 lifecycle events for one project.

    Args:
        project_id (UUID):
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | Error]
     """


    kwargs = _get_kwargs(
        project_id=project_id,
x_open_geo_project=x_open_geo_project,

    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)

def sync(
    project_id: UUID,
    *,
    client: AuthenticatedClient | Client,
    x_open_geo_project: str | Unset = UNSET,

) -> Any | Error | None:
    """ Server-Sent Events stream of ARCH-17 lifecycle events for one project.

    Args:
        project_id (UUID):
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | Error
     """


    return sync_detailed(
        project_id=project_id,
client=client,
x_open_geo_project=x_open_geo_project,

    ).parsed

async def asyncio_detailed(
    project_id: UUID,
    *,
    client: AuthenticatedClient | Client,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[Any | Error]:
    """ Server-Sent Events stream of ARCH-17 lifecycle events for one project.

    Args:
        project_id (UUID):
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | Error]
     """


    kwargs = _get_kwargs(
        project_id=project_id,
x_open_geo_project=x_open_geo_project,

    )

    response = await client.get_async_httpx_client().request(
        **kwargs
    )

    return _build_response(client=client, response=response)

async def asyncio(
    project_id: UUID,
    *,
    client: AuthenticatedClient | Client,
    x_open_geo_project: str | Unset = UNSET,

) -> Any | Error | None:
    """ Server-Sent Events stream of ARCH-17 lifecycle events for one project.

    Args:
        project_id (UUID):
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | Error
     """


    return (await asyncio_detailed(
        project_id=project_id,
client=client,
x_open_geo_project=x_open_geo_project,

    )).parsed
