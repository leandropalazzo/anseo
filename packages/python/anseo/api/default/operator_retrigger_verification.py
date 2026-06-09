from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.operator_retrigger_verification_body import (
    OperatorRetriggerVerificationBody,
)
from ...models.operator_retrigger_verification_response_200 import (
    OperatorRetriggerVerificationResponse200,
)
from ...types import Response


def _get_kwargs(
    domain: str,
    *,
    body: OperatorRetriggerVerificationBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/operator/entities/{domain}/retrigger".format(
            domain=quote(str(domain), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | OperatorRetriggerVerificationResponse200 | None:
    if response.status_code == 200:
        response_200 = OperatorRetriggerVerificationResponse200.from_dict(
            response.json()
        )

        return response_200

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if response.status_code == 403:
        response_403 = Error.from_dict(response.json())

        return response_403

    if response.status_code == 404:
        response_404 = Error.from_dict(response.json())

        return response_404

    if response.status_code == 502:
        response_502 = Error.from_dict(response.json())

        return response_502

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Error | OperatorRetriggerVerificationResponse200]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    domain: str,
    *,
    client: AuthenticatedClient | Client,
    body: OperatorRetriggerVerificationBody,
) -> Response[Error | OperatorRetriggerVerificationResponse200]:
    """Story 48.4 — re-issue the 43.2 email magic-link verification (mint a fresh challenge + re-send via
    the comms SMTP wire). Honest failure: if mail is not configured the call returns 502 (no silent
    success). Operator-scoped; tenant keys 403.

    Args:
        domain (str):
        body (OperatorRetriggerVerificationBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorRetriggerVerificationResponse200]
    """

    kwargs = _get_kwargs(
        domain=domain,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    domain: str,
    *,
    client: AuthenticatedClient | Client,
    body: OperatorRetriggerVerificationBody,
) -> Error | OperatorRetriggerVerificationResponse200 | None:
    """Story 48.4 — re-issue the 43.2 email magic-link verification (mint a fresh challenge + re-send via
    the comms SMTP wire). Honest failure: if mail is not configured the call returns 502 (no silent
    success). Operator-scoped; tenant keys 403.

    Args:
        domain (str):
        body (OperatorRetriggerVerificationBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorRetriggerVerificationResponse200
    """

    return sync_detailed(
        domain=domain,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    domain: str,
    *,
    client: AuthenticatedClient | Client,
    body: OperatorRetriggerVerificationBody,
) -> Response[Error | OperatorRetriggerVerificationResponse200]:
    """Story 48.4 — re-issue the 43.2 email magic-link verification (mint a fresh challenge + re-send via
    the comms SMTP wire). Honest failure: if mail is not configured the call returns 502 (no silent
    success). Operator-scoped; tenant keys 403.

    Args:
        domain (str):
        body (OperatorRetriggerVerificationBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorRetriggerVerificationResponse200]
    """

    kwargs = _get_kwargs(
        domain=domain,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    domain: str,
    *,
    client: AuthenticatedClient | Client,
    body: OperatorRetriggerVerificationBody,
) -> Error | OperatorRetriggerVerificationResponse200 | None:
    """Story 48.4 — re-issue the 43.2 email magic-link verification (mint a fresh challenge + re-send via
    the comms SMTP wire). Honest failure: if mail is not configured the call returns 502 (no silent
    success). Operator-scoped; tenant keys 403.

    Args:
        domain (str):
        body (OperatorRetriggerVerificationBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorRetriggerVerificationResponse200
    """

    return (
        await asyncio_detailed(
            domain=domain,
            client=client,
            body=body,
        )
    ).parsed
