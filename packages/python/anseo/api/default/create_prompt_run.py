from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.create_prompt_run_request import CreatePromptRunRequest
from ...models.create_prompt_run_response import CreatePromptRunResponse
from ...models.error import Error
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    body: CreatePromptRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/prompt-runs",
    }

    _body = body.to_dict()

    _kwargs["json"] = _body
    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[CreatePromptRunResponse, Error]]:
    if response.status_code == 202:
        response_202 = CreatePromptRunResponse.from_dict(response.json())

        return response_202
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
) -> Response[Union[CreatePromptRunResponse, Error]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    body: CreatePromptRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[CreatePromptRunResponse, Error]]:
    """Dispatch a one-shot prompt run for an already-declared Prompt and Provider.

    Args:
        x_anseo_project (Union[Unset, str]):
        body (CreatePromptRunRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[CreatePromptRunResponse, Error]]
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
    body: CreatePromptRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[CreatePromptRunResponse, Error]]:
    """Dispatch a one-shot prompt run for an already-declared Prompt and Provider.

    Args:
        x_anseo_project (Union[Unset, str]):
        body (CreatePromptRunRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[CreatePromptRunResponse, Error]
    """

    return sync_detailed(
        client=client,
        body=body,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: Union[AuthenticatedClient, Client],
    body: CreatePromptRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[CreatePromptRunResponse, Error]]:
    """Dispatch a one-shot prompt run for an already-declared Prompt and Provider.

    Args:
        x_anseo_project (Union[Unset, str]):
        body (CreatePromptRunRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[CreatePromptRunResponse, Error]]
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
    body: CreatePromptRunRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[CreatePromptRunResponse, Error]]:
    """Dispatch a one-shot prompt run for an already-declared Prompt and Provider.

    Args:
        x_anseo_project (Union[Unset, str]):
        body (CreatePromptRunRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[CreatePromptRunResponse, Error]
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
