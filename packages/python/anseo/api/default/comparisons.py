from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.comparisons_response import ComparisonsResponse
from ...models.comparisons_window import ComparisonsWindow
from ...models.error import Error
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    brands: str,
    prompts: Union[Unset, str] = UNSET,
    providers: Union[Unset, str] = UNSET,
    window: Union[Unset, ComparisonsWindow] = ComparisonsWindow.VALUE_1,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    params: dict[str, Any] = {}

    params["brands"] = brands

    params["prompts"] = prompts

    params["providers"] = providers

    json_window: Union[Unset, str] = UNSET
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


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[ComparisonsResponse, Error]]:
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


def _build_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Response[Union[ComparisonsResponse, Error]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    brands: str,
    prompts: Union[Unset, str] = UNSET,
    providers: Union[Unset, str] = UNSET,
    window: Union[Unset, ComparisonsWindow] = ComparisonsWindow.VALUE_1,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[ComparisonsResponse, Error]]:
    """Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP
    `compare_brands`).

    Args:
        brands (str): Comma-separated; 2..=6 entries. First entry is the subject brand; remainder
            are competitors in caller-declared order.
        prompts (Union[Unset, str]): Comma-separated prompt names; default = all prompts for the
            project.
        providers (Union[Unset, str]): Comma-separated provider names; default = all providers.
        window (Union[Unset, ComparisonsWindow]):  Default: ComparisonsWindow.VALUE_1.
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[ComparisonsResponse, Error]]
    """

    kwargs = _get_kwargs(
        brands=brands,
        prompts=prompts,
        providers=providers,
        window=window,
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: Union[AuthenticatedClient, Client],
    brands: str,
    prompts: Union[Unset, str] = UNSET,
    providers: Union[Unset, str] = UNSET,
    window: Union[Unset, ComparisonsWindow] = ComparisonsWindow.VALUE_1,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[ComparisonsResponse, Error]]:
    """Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP
    `compare_brands`).

    Args:
        brands (str): Comma-separated; 2..=6 entries. First entry is the subject brand; remainder
            are competitors in caller-declared order.
        prompts (Union[Unset, str]): Comma-separated prompt names; default = all prompts for the
            project.
        providers (Union[Unset, str]): Comma-separated provider names; default = all providers.
        window (Union[Unset, ComparisonsWindow]):  Default: ComparisonsWindow.VALUE_1.
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[ComparisonsResponse, Error]
    """

    return sync_detailed(
        client=client,
        brands=brands,
        prompts=prompts,
        providers=providers,
        window=window,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    brands: str,
    prompts: Union[Unset, str] = UNSET,
    providers: Union[Unset, str] = UNSET,
    window: Union[Unset, ComparisonsWindow] = ComparisonsWindow.VALUE_1,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[ComparisonsResponse, Error]]:
    """Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP
    `compare_brands`).

    Args:
        brands (str): Comma-separated; 2..=6 entries. First entry is the subject brand; remainder
            are competitors in caller-declared order.
        prompts (Union[Unset, str]): Comma-separated prompt names; default = all prompts for the
            project.
        providers (Union[Unset, str]): Comma-separated provider names; default = all providers.
        window (Union[Unset, ComparisonsWindow]):  Default: ComparisonsWindow.VALUE_1.
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[ComparisonsResponse, Error]]
    """

    kwargs = _get_kwargs(
        brands=brands,
        prompts=prompts,
        providers=providers,
        window=window,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: Union[AuthenticatedClient, Client],
    brands: str,
    prompts: Union[Unset, str] = UNSET,
    providers: Union[Unset, str] = UNSET,
    window: Union[Unset, ComparisonsWindow] = ComparisonsWindow.VALUE_1,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[ComparisonsResponse, Error]]:
    """Phase 3 Story 0.8 — deterministic brand-vs-competitors comparison matrix (substrate for MCP
    `compare_brands`).

    Args:
        brands (str): Comma-separated; 2..=6 entries. First entry is the subject brand; remainder
            are competitors in caller-declared order.
        prompts (Union[Unset, str]): Comma-separated prompt names; default = all prompts for the
            project.
        providers (Union[Unset, str]): Comma-separated provider names; default = all providers.
        window (Union[Unset, ComparisonsWindow]):  Default: ComparisonsWindow.VALUE_1.
        x_anseo_project (Union[Unset, str]):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[ComparisonsResponse, Error]
    """

    return (
        await asyncio_detailed(
            client=client,
            brands=brands,
            prompts=prompts,
            providers=providers,
            window=window,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
