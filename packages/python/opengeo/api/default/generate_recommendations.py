from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.error import Error
from ...models.generate_recommendations_accepted import GenerateRecommendationsAccepted
from ...types import UNSET, Unset
from typing import cast



def _get_kwargs(
    *,
    x_open_geo_project: str | Unset = UNSET,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_open_geo_project, Unset):
        headers["X-OpenGEO-Project"] = x_open_geo_project



    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/recommendations/generate",
    }


    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Error | GenerateRecommendationsAccepted | None:
    if response.status_code == 202:
        response_202 = GenerateRecommendationsAccepted.from_dict(response.json())



        return response_202

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())



        return response_401

    if response.status_code == 503:
        response_503 = Error.from_dict(response.json())



        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Error | GenerateRecommendationsAccepted]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[Error | GenerateRecommendationsAccepted]:
    """ Story 19.6 — assemble an EngineInput from the project's live prompts/runs/citations, run the in-
    process engine, and persist the result. Returns 202 + a status_url per the Phase 2 async-write
    pattern.

    Args:
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | GenerateRecommendationsAccepted]
     """


    kwargs = _get_kwargs(
        x_open_geo_project=x_open_geo_project,

    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)

def sync(
    *,
    client: AuthenticatedClient | Client,
    x_open_geo_project: str | Unset = UNSET,

) -> Error | GenerateRecommendationsAccepted | None:
    """ Story 19.6 — assemble an EngineInput from the project's live prompts/runs/citations, run the in-
    process engine, and persist the result. Returns 202 + a status_url per the Phase 2 async-write
    pattern.

    Args:
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | GenerateRecommendationsAccepted
     """


    return sync_detailed(
        client=client,
x_open_geo_project=x_open_geo_project,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    x_open_geo_project: str | Unset = UNSET,

) -> Response[Error | GenerateRecommendationsAccepted]:
    """ Story 19.6 — assemble an EngineInput from the project's live prompts/runs/citations, run the in-
    process engine, and persist the result. Returns 202 + a status_url per the Phase 2 async-write
    pattern.

    Args:
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | GenerateRecommendationsAccepted]
     """


    kwargs = _get_kwargs(
        x_open_geo_project=x_open_geo_project,

    )

    response = await client.get_async_httpx_client().request(
        **kwargs
    )

    return _build_response(client=client, response=response)

async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    x_open_geo_project: str | Unset = UNSET,

) -> Error | GenerateRecommendationsAccepted | None:
    """ Story 19.6 — assemble an EngineInput from the project's live prompts/runs/citations, run the in-
    process engine, and persist the result. Returns 202 + a status_url per the Phase 2 async-write
    pattern.

    Args:
        x_open_geo_project (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | GenerateRecommendationsAccepted
     """


    return (await asyncio_detailed(
        client=client,
x_open_geo_project=x_open_geo_project,

    )).parsed
