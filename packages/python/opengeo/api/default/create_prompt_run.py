from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.create_prompt_run_request import CreatePromptRunRequest
from ...models.create_prompt_run_response import CreatePromptRunResponse
from ...models.error import Error
from ...types import UNSET, Unset
from typing import cast



def _get_kwargs(
    *,
    body: CreatePromptRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_open_geo_project, Unset):
        headers["X-OpenGEO-Project"] = x_open_geo_project



    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/prompt-runs",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> CreatePromptRunResponse | Error | None:
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


def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[CreatePromptRunResponse | Error]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: CreatePromptRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[CreatePromptRunResponse | Error]:
    """ Dispatch a one-shot prompt run for an already-declared Prompt and Provider.

    Args:
        x_open_geo_project (str | Unset):
        body (CreatePromptRunRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[CreatePromptRunResponse | Error]
     """


    kwargs = _get_kwargs(
        body=body,
x_open_geo_project=x_open_geo_project,

    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)

def sync(
    *,
    client: AuthenticatedClient | Client,
    body: CreatePromptRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> CreatePromptRunResponse | Error | None:
    """ Dispatch a one-shot prompt run for an already-declared Prompt and Provider.

    Args:
        x_open_geo_project (str | Unset):
        body (CreatePromptRunRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        CreatePromptRunResponse | Error
     """


    return sync_detailed(
        client=client,
body=body,
x_open_geo_project=x_open_geo_project,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: CreatePromptRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[CreatePromptRunResponse | Error]:
    """ Dispatch a one-shot prompt run for an already-declared Prompt and Provider.

    Args:
        x_open_geo_project (str | Unset):
        body (CreatePromptRunRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[CreatePromptRunResponse | Error]
     """


    kwargs = _get_kwargs(
        body=body,
x_open_geo_project=x_open_geo_project,

    )

    response = await client.get_async_httpx_client().request(
        **kwargs
    )

    return _build_response(client=client, response=response)

async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: CreatePromptRunRequest,
    x_open_geo_project: str | Unset = UNSET,

) -> CreatePromptRunResponse | Error | None:
    """ Dispatch a one-shot prompt run for an already-declared Prompt and Provider.

    Args:
        x_open_geo_project (str | Unset):
        body (CreatePromptRunRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        CreatePromptRunResponse | Error
     """


    return (await asyncio_detailed(
        client=client,
body=body,
x_open_geo_project=x_open_geo_project,

    )).parsed
