from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.operator_erase_entity_body import OperatorEraseEntityBody
from ...models.operator_erase_entity_response_200 import OperatorEraseEntityResponse200
from ...types import UNSET, Response, Unset


def _get_kwargs(
    domain: str,
    *,
    body: OperatorEraseEntityBody | Unset = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/operator/entities/{domain}/erase".format(
            domain=quote(str(domain), safe=""),
        ),
    }

    if not isinstance(body, Unset):
        _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | OperatorEraseEntityResponse200 | None:
    if response.status_code == 200:
        response_200 = OperatorEraseEntityResponse200.from_dict(response.json())

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

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Error | OperatorEraseEntityResponse200]:
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
    body: OperatorEraseEntityBody | Unset = UNSET,
) -> Response[Error | OperatorEraseEntityResponse200]:
    """Story 48.4 — GDPR erase (two-step, irreversible). A call with no confirm_token returns a short-lived
    (~5 min) signed HMAC confirm token bound to (domain, actor) and erases NOTHING; a call presenting
    the matching token transactionally deletes the entity + its verification_attempts + identifiable
    dispute rows. KEK SAFETY: ProjectKek::destroy (crypto-shred) runs ONLY when the domain maps to
    EXACTLY ONE project via the identified-contribution linkage; otherwise the response carries
    kek_destroyed:false with a kek_skip_reason (we never destroy a KEK that could shred unrelated
    contributors). Operator-scoped; tenant keys 403.

    Args:
        domain (str):
        body (OperatorEraseEntityBody | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorEraseEntityResponse200]
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
    body: OperatorEraseEntityBody | Unset = UNSET,
) -> Error | OperatorEraseEntityResponse200 | None:
    """Story 48.4 — GDPR erase (two-step, irreversible). A call with no confirm_token returns a short-lived
    (~5 min) signed HMAC confirm token bound to (domain, actor) and erases NOTHING; a call presenting
    the matching token transactionally deletes the entity + its verification_attempts + identifiable
    dispute rows. KEK SAFETY: ProjectKek::destroy (crypto-shred) runs ONLY when the domain maps to
    EXACTLY ONE project via the identified-contribution linkage; otherwise the response carries
    kek_destroyed:false with a kek_skip_reason (we never destroy a KEK that could shred unrelated
    contributors). Operator-scoped; tenant keys 403.

    Args:
        domain (str):
        body (OperatorEraseEntityBody | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorEraseEntityResponse200
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
    body: OperatorEraseEntityBody | Unset = UNSET,
) -> Response[Error | OperatorEraseEntityResponse200]:
    """Story 48.4 — GDPR erase (two-step, irreversible). A call with no confirm_token returns a short-lived
    (~5 min) signed HMAC confirm token bound to (domain, actor) and erases NOTHING; a call presenting
    the matching token transactionally deletes the entity + its verification_attempts + identifiable
    dispute rows. KEK SAFETY: ProjectKek::destroy (crypto-shred) runs ONLY when the domain maps to
    EXACTLY ONE project via the identified-contribution linkage; otherwise the response carries
    kek_destroyed:false with a kek_skip_reason (we never destroy a KEK that could shred unrelated
    contributors). Operator-scoped; tenant keys 403.

    Args:
        domain (str):
        body (OperatorEraseEntityBody | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorEraseEntityResponse200]
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
    body: OperatorEraseEntityBody | Unset = UNSET,
) -> Error | OperatorEraseEntityResponse200 | None:
    """Story 48.4 — GDPR erase (two-step, irreversible). A call with no confirm_token returns a short-lived
    (~5 min) signed HMAC confirm token bound to (domain, actor) and erases NOTHING; a call presenting
    the matching token transactionally deletes the entity + its verification_attempts + identifiable
    dispute rows. KEK SAFETY: ProjectKek::destroy (crypto-shred) runs ONLY when the domain maps to
    EXACTLY ONE project via the identified-contribution linkage; otherwise the response carries
    kek_destroyed:false with a kek_skip_reason (we never destroy a KEK that could shred unrelated
    contributors). Operator-scoped; tenant keys 403.

    Args:
        domain (str):
        body (OperatorEraseEntityBody | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorEraseEntityResponse200
    """

    return (
        await asyncio_detailed(
            domain=domain,
            client=client,
            body=body,
        )
    ).parsed
