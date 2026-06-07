from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.brand_update import BrandUpdate
from ...models.brand_update_result import BrandUpdateResult
from ...models.error import Error
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    body: BrandUpdate,
    x_anseo_project: str | Unset = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/v1/setup/brand",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> BrandUpdateResult | Error | None:
    if response.status_code == 200:
        response_200 = BrandUpdateResult.from_dict(response.json())

        return response_200

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if response.status_code == 409:
        response_409 = Error.from_dict(response.json())

        return response_409

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[BrandUpdateResult | Error]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: BrandUpdate,
    x_anseo_project: str | Unset = UNSET,
) -> Response[BrandUpdateResult | Error]:
    """Update brand config. Changing the name re-derives project_id (allowed only before the first run) and
    sets restart_required.

    Args:
        x_anseo_project (str | Unset):
        body (BrandUpdate):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BrandUpdateResult | Error]
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
    client: AuthenticatedClient | Client,
    body: BrandUpdate,
    x_anseo_project: str | Unset = UNSET,
) -> BrandUpdateResult | Error | None:
    """Update brand config. Changing the name re-derives project_id (allowed only before the first run) and
    sets restart_required.

    Args:
        x_anseo_project (str | Unset):
        body (BrandUpdate):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BrandUpdateResult | Error
    """

    return sync_detailed(
        client=client,
        body=body,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: BrandUpdate,
    x_anseo_project: str | Unset = UNSET,
) -> Response[BrandUpdateResult | Error]:
    """Update brand config. Changing the name re-derives project_id (allowed only before the first run) and
    sets restart_required.

    Args:
        x_anseo_project (str | Unset):
        body (BrandUpdate):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BrandUpdateResult | Error]
    """

    kwargs = _get_kwargs(
        body=body,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: BrandUpdate,
    x_anseo_project: str | Unset = UNSET,
) -> BrandUpdateResult | Error | None:
    """Update brand config. Changing the name re-derives project_id (allowed only before the first run) and
    sets restart_required.

    Args:
        x_anseo_project (str | Unset):
        body (BrandUpdate):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BrandUpdateResult | Error
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
