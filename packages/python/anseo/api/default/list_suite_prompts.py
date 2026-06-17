from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.suite_prompt_summary import SuitePromptSummary
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/suite/prompts",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | list[SuitePromptSummary] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = SuitePromptSummary.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Error | list[SuitePromptSummary]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[Error | list[SuitePromptSummary]]:
    """Story 40.5 — list the canonical GEO benchmark prompt slugs (`slug`, `version`, `description`)
    external instrumentation should reuse for comparable contribution cohorts. Operator-scoped global
    metadata; not gated by X-Anseo-Project.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | list[SuitePromptSummary]]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> Error | list[SuitePromptSummary] | None:
    """Story 40.5 — list the canonical GEO benchmark prompt slugs (`slug`, `version`, `description`)
    external instrumentation should reuse for comparable contribution cohorts. Operator-scoped global
    metadata; not gated by X-Anseo-Project.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | list[SuitePromptSummary]
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[Error | list[SuitePromptSummary]]:
    """Story 40.5 — list the canonical GEO benchmark prompt slugs (`slug`, `version`, `description`)
    external instrumentation should reuse for comparable contribution cohorts. Operator-scoped global
    metadata; not gated by X-Anseo-Project.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | list[SuitePromptSummary]]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> Error | list[SuitePromptSummary] | None:
    """Story 40.5 — list the canonical GEO benchmark prompt slugs (`slug`, `version`, `description`)
    external instrumentation should reuse for comparable contribution cohorts. Operator-scoped global
    metadata; not gated by X-Anseo-Project.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | list[SuitePromptSummary]
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
