from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.error import Error
from ...models.install_plugin_body import InstallPluginBody
from ...models.install_plugin_response_200 import InstallPluginResponse200
from typing import cast



def _get_kwargs(
    *,
    body: InstallPluginBody,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}


    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/plugins/install",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Error | InstallPluginResponse200 | None:
    if response.status_code == 200:
        response_200 = InstallPluginResponse200.from_dict(response.json())



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


def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Error | InstallPluginResponse200]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: InstallPluginBody,

) -> Response[Error | InstallPluginResponse200]:
    """ Story 41.3 — verify (checksum + Ed25519 signature) and record a plugin install from the live
    registry by id. Operator-scoped; not gated by X-Anseo-Project.

    Args:
        body (InstallPluginBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | InstallPluginResponse200]
     """


    kwargs = _get_kwargs(
        body=body,

    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)

def sync(
    *,
    client: AuthenticatedClient | Client,
    body: InstallPluginBody,

) -> Error | InstallPluginResponse200 | None:
    """ Story 41.3 — verify (checksum + Ed25519 signature) and record a plugin install from the live
    registry by id. Operator-scoped; not gated by X-Anseo-Project.

    Args:
        body (InstallPluginBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | InstallPluginResponse200
     """


    return sync_detailed(
        client=client,
body=body,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: InstallPluginBody,

) -> Response[Error | InstallPluginResponse200]:
    """ Story 41.3 — verify (checksum + Ed25519 signature) and record a plugin install from the live
    registry by id. Operator-scoped; not gated by X-Anseo-Project.

    Args:
        body (InstallPluginBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | InstallPluginResponse200]
     """


    kwargs = _get_kwargs(
        body=body,

    )

    response = await client.get_async_httpx_client().request(
        **kwargs
    )

    return _build_response(client=client, response=response)

async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: InstallPluginBody,

) -> Error | InstallPluginResponse200 | None:
    """ Story 41.3 — verify (checksum + Ed25519 signature) and record a plugin install from the live
    registry by id. Operator-scoped; not gated by X-Anseo-Project.

    Args:
        body (InstallPluginBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | InstallPluginResponse200
     """


    return (await asyncio_detailed(
        client=client,
body=body,

    )).parsed
