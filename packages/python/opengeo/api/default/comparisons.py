from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.comparisons_response import ComparisonsResponse
from ...models.comparisons_window import ComparisonsWindow
from ...models.error import Error
from ...types import UNSET, Unset
from typing import cast



def _get_kwargs(
    *,
    brands: str,
    prompts: str | Unset = UNSET,
    providers: str | Unset = UNSET,
    window: ComparisonsWindow | Unset = ComparisonsWindow.VALUE_1,
    x_open_geo_project: str | Unset = UNSET,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_open_geo_project, Unset):
        headers["X-OpenGEO-Project"] = x_open_geo_project



    

    params: dict[str, Any] = {}

    params["brands"] = brands

    params["prompts"] = prompts

    params["providers"] = providers

    json_window: str | Unset = UNSET
    if not isinstance(window, Unset):
        json_window = window.value

    params["window"] = json_window


    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}


    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/comparisons",
        "params": params,
    }


    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> ComparisonsResponse | Error | None:
    if response.status_code == 200:
        response_200 = ComparisonsResponse.from_dict(response.json())



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


def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[ComparisonsResponse | Error]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    brands: str,
    prompts: str | Unset = UNSET,
    providers: str | Unset = UNSET,
    window: ComparisonsWindow | Unset = ComparisonsWindow.VALUE_1,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[ComparisonsResponse | Error]:
    """ Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP
    `compare_brands`).

    Args:
        brands (str): Comma-separated; 2..=6 entries. First entry is the subject brand; remainder
            are competitors in caller-declared order.
        prompts (str | Unset): Comma-separated prompt names; default = all prompts for the
            project.
        providers (str | Unset): Comma-separated provider names; default = all providers.
        window (ComparisonsWindow | Unset):  Default: ComparisonsWindow.VALUE_1.
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ComparisonsResponse | Error]
     """


    kwargs = _get_kwargs(
        brands=brands,
prompts=prompts,
providers=providers,
window=window,
x_open_geo_project=x_open_geo_project,

    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)

def sync(
    *,
    client: AuthenticatedClient | Client,
    brands: str,
    prompts: str | Unset = UNSET,
    providers: str | Unset = UNSET,
    window: ComparisonsWindow | Unset = ComparisonsWindow.VALUE_1,
    x_open_geo_project: str | Unset = UNSET,

) -> ComparisonsResponse | Error | None:
    """ Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP
    `compare_brands`).

    Args:
        brands (str): Comma-separated; 2..=6 entries. First entry is the subject brand; remainder
            are competitors in caller-declared order.
        prompts (str | Unset): Comma-separated prompt names; default = all prompts for the
            project.
        providers (str | Unset): Comma-separated provider names; default = all providers.
        window (ComparisonsWindow | Unset):  Default: ComparisonsWindow.VALUE_1.
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ComparisonsResponse | Error
     """


    return sync_detailed(
        client=client,
brands=brands,
prompts=prompts,
providers=providers,
window=window,
x_open_geo_project=x_open_geo_project,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    brands: str,
    prompts: str | Unset = UNSET,
    providers: str | Unset = UNSET,
    window: ComparisonsWindow | Unset = ComparisonsWindow.VALUE_1,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[ComparisonsResponse | Error]:
    """ Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP
    `compare_brands`).

    Args:
        brands (str): Comma-separated; 2..=6 entries. First entry is the subject brand; remainder
            are competitors in caller-declared order.
        prompts (str | Unset): Comma-separated prompt names; default = all prompts for the
            project.
        providers (str | Unset): Comma-separated provider names; default = all providers.
        window (ComparisonsWindow | Unset):  Default: ComparisonsWindow.VALUE_1.
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ComparisonsResponse | Error]
     """


    kwargs = _get_kwargs(
        brands=brands,
prompts=prompts,
providers=providers,
window=window,
x_open_geo_project=x_open_geo_project,

    )

    response = await client.get_async_httpx_client().request(
        **kwargs
    )

    return _build_response(client=client, response=response)

async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    brands: str,
    prompts: str | Unset = UNSET,
    providers: str | Unset = UNSET,
    window: ComparisonsWindow | Unset = ComparisonsWindow.VALUE_1,
    x_open_geo_project: str | Unset = UNSET,

) -> ComparisonsResponse | Error | None:
    """ Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP
    `compare_brands`).

    Args:
        brands (str): Comma-separated; 2..=6 entries. First entry is the subject brand; remainder
            are competitors in caller-declared order.
        prompts (str | Unset): Comma-separated prompt names; default = all prompts for the
            project.
        providers (str | Unset): Comma-separated provider names; default = all providers.
        window (ComparisonsWindow | Unset):  Default: ComparisonsWindow.VALUE_1.
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ComparisonsResponse | Error
     """


    return (await asyncio_detailed(
        client=client,
brands=brands,
prompts=prompts,
providers=providers,
window=window,
x_open_geo_project=x_open_geo_project,

    )).parsed
