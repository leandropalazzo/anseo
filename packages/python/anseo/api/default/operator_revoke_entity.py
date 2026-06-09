from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.operator_entity import OperatorEntity
from ...models.operator_revoke_entity_body import OperatorRevokeEntityBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    domain: str,
    *,
    body: OperatorRevokeEntityBody | Unset = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/operator/entities/{domain}/revoke".format(
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
) -> Error | OperatorEntity | None:
    if response.status_code == 200:
        response_200 = OperatorEntity.from_dict(response.json())

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
) -> Response[Error | OperatorEntity]:
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
    body: OperatorRevokeEntityBody | Unset = UNSET,
) -> Response[Error | OperatorEntity]:
    """Story 48.4 — revoke a claimed entity via the SHARED revoke path (set claim_status=revoked + start
    14-day grace period + append revocation ledger row) — the same path the daily re-verify job uses,
    not a fork. Actor read from X-Anseo-Operator-Actor / operator body. Operator-scoped; tenant keys
    403.

    Args:
        domain (str):
        body (OperatorRevokeEntityBody | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorEntity]
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
    body: OperatorRevokeEntityBody | Unset = UNSET,
) -> Error | OperatorEntity | None:
    """Story 48.4 — revoke a claimed entity via the SHARED revoke path (set claim_status=revoked + start
    14-day grace period + append revocation ledger row) — the same path the daily re-verify job uses,
    not a fork. Actor read from X-Anseo-Operator-Actor / operator body. Operator-scoped; tenant keys
    403.

    Args:
        domain (str):
        body (OperatorRevokeEntityBody | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorEntity
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
    body: OperatorRevokeEntityBody | Unset = UNSET,
) -> Response[Error | OperatorEntity]:
    """Story 48.4 — revoke a claimed entity via the SHARED revoke path (set claim_status=revoked + start
    14-day grace period + append revocation ledger row) — the same path the daily re-verify job uses,
    not a fork. Actor read from X-Anseo-Operator-Actor / operator body. Operator-scoped; tenant keys
    403.

    Args:
        domain (str):
        body (OperatorRevokeEntityBody | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorEntity]
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
    body: OperatorRevokeEntityBody | Unset = UNSET,
) -> Error | OperatorEntity | None:
    """Story 48.4 — revoke a claimed entity via the SHARED revoke path (set claim_status=revoked + start
    14-day grace period + append revocation ledger row) — the same path the daily re-verify job uses,
    not a fork. Actor read from X-Anseo-Operator-Actor / operator body. Operator-scoped; tenant keys
    403.

    Args:
        domain (str):
        body (OperatorRevokeEntityBody | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorEntity
    """

    return (
        await asyncio_detailed(
            domain=domain,
            client=client,
            body=body,
        )
    ).parsed
