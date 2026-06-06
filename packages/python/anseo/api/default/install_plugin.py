from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.install_plugin_body import InstallPluginBody
from ...models.install_plugin_response_200 import InstallPluginResponse200
from ...types import Response


def _get_kwargs(
    *,
    body: InstallPluginBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/plugins/install",
    }

    _body = body.to_dict()

    _kwargs["json"] = _body
    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[Error, InstallPluginResponse200]]:
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


def _build_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Response[Union[Error, InstallPluginResponse200]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    body: InstallPluginBody,
) -> Response[Union[Error, InstallPluginResponse200]]:
    """Story 41.3 — verify (checksum + Ed25519 signature) and record a plugin install from the live
    registry by id. Operator-scoped; not gated by X-Anseo-Project.

    Args:
        body (InstallPluginBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, InstallPluginResponse200]]
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
    client: Union[AuthenticatedClient, Client],
    body: InstallPluginBody,
) -> Optional[Union[Error, InstallPluginResponse200]]:
    """Story 41.3 — verify (checksum + Ed25519 signature) and record a plugin install from the live
    registry by id. Operator-scoped; not gated by X-Anseo-Project.

    Args:
        body (InstallPluginBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, InstallPluginResponse200]
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    body: InstallPluginBody,
) -> Response[Union[Error, InstallPluginResponse200]]:
    """Story 41.3 — verify (checksum + Ed25519 signature) and record a plugin install from the live
    registry by id. Operator-scoped; not gated by X-Anseo-Project.

    Args:
        body (InstallPluginBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, InstallPluginResponse200]]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: Union[AuthenticatedClient, Client],
    body: InstallPluginBody,
) -> Optional[Union[Error, InstallPluginResponse200]]:
    """Story 41.3 — verify (checksum + Ed25519 signature) and record a plugin install from the live
    registry by id. Operator-scoped; not gated by X-Anseo-Project.

    Args:
        body (InstallPluginBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, InstallPluginResponse200]
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
