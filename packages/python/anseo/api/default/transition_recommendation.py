from http import HTTPStatus
from typing import Any, Optional, Union

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.transition_recommendation_request import TransitionRecommendationRequest
from ...models.transition_recommendation_response import (
    TransitionRecommendationResponse,
)
from ...types import UNSET, Response, Unset


def _get_kwargs(
    id: str,
    *,
    body: TransitionRecommendationRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    _kwargs: dict[str, Any] = {
        "method": "patch",
        "url": "/v1/recommendations/{id}/state".format(
            id=id,
        ),
    }

    _body = body.to_dict()

    _kwargs["json"] = _body
    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Optional[Union[Error, TransitionRecommendationResponse]]:
    if response.status_code == 200:
        response_200 = TransitionRecommendationResponse.from_dict(response.json())

        return response_200
    if response.status_code == 400:
        response_400 = Error.from_dict(response.json())

        return response_400
    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401
    if response.status_code == 404:
        response_404 = Error.from_dict(response.json())

        return response_404
    if response.status_code == 409:
        response_409 = Error.from_dict(response.json())

        return response_409
    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: Union[AuthenticatedClient, Client], response: httpx.Response
) -> Response[Union[Error, TransitionRecommendationResponse]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    id: str,
    *,
    client: Union[AuthenticatedClient, Client],
    body: TransitionRecommendationRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[Error, TransitionRecommendationResponse]]:
    """Story 19.6 — apply a lifecycle transition (Story 19.4 state machine). Illegal transitions return
    409.

    Args:
        id (str): Recommendation ULID.
        x_anseo_project (Union[Unset, str]):
        body (TransitionRecommendationRequest): Story 19.6 — lifecycle transition (Story 19.4
            state machine). Illegal edges return 409.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, TransitionRecommendationResponse]]
    """

    kwargs = _get_kwargs(
        id=id,
        body=body,
        x_anseo_project=x_anseo_project,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: Union[AuthenticatedClient, Client],
    body: TransitionRecommendationRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[Error, TransitionRecommendationResponse]]:
    """Story 19.6 — apply a lifecycle transition (Story 19.4 state machine). Illegal transitions return
    409.

    Args:
        id (str): Recommendation ULID.
        x_anseo_project (Union[Unset, str]):
        body (TransitionRecommendationRequest): Story 19.6 — lifecycle transition (Story 19.4
            state machine). Illegal edges return 409.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, TransitionRecommendationResponse]
    """

    return sync_detailed(
        id=id,
        client=client,
        body=body,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: Union[AuthenticatedClient, Client],
    body: TransitionRecommendationRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Response[Union[Error, TransitionRecommendationResponse]]:
    """Story 19.6 — apply a lifecycle transition (Story 19.4 state machine). Illegal transitions return
    409.

    Args:
        id (str): Recommendation ULID.
        x_anseo_project (Union[Unset, str]):
        body (TransitionRecommendationRequest): Story 19.6 — lifecycle transition (Story 19.4
            state machine). Illegal edges return 409.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Union[Error, TransitionRecommendationResponse]]
    """

    kwargs = _get_kwargs(
        id=id,
        body=body,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: Union[AuthenticatedClient, Client],
    body: TransitionRecommendationRequest,
    x_anseo_project: Union[Unset, str] = UNSET,
) -> Optional[Union[Error, TransitionRecommendationResponse]]:
    """Story 19.6 — apply a lifecycle transition (Story 19.4 state machine). Illegal transitions return
    409.

    Args:
        id (str): Recommendation ULID.
        x_anseo_project (Union[Unset, str]):
        body (TransitionRecommendationRequest): Story 19.6 — lifecycle transition (Story 19.4
            state machine). Illegal edges return 409.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Union[Error, TransitionRecommendationResponse]
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            body=body,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
