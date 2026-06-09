from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.operator_list_entities_claim_status import (
    OperatorListEntitiesClaimStatus,
)
from ...models.operator_list_entities_response_200 import (
    OperatorListEntitiesResponse200,
)
from ...models.operator_list_entities_verification_method import (
    OperatorListEntitiesVerificationMethod,
)
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    claim_status: OperatorListEntitiesClaimStatus | Unset = UNSET,
    verification_method: OperatorListEntitiesVerificationMethod | Unset = UNSET,
    domain: str | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    json_claim_status: str | Unset = UNSET
    if not isinstance(claim_status, Unset):
        json_claim_status = claim_status.value

    params["claim_status"] = json_claim_status

    json_verification_method: str | Unset = UNSET
    if not isinstance(verification_method, Unset):
        json_verification_method = verification_method.value

    params["verification_method"] = json_verification_method

    params["domain"] = domain

    params["limit"] = limit

    params["offset"] = offset

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/operator/entities",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | OperatorListEntitiesResponse200 | None:
    if response.status_code == 200:
        response_200 = OperatorListEntitiesResponse200.from_dict(response.json())

        return response_200

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if response.status_code == 403:
        response_403 = Error.from_dict(response.json())

        return response_403

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Error | OperatorListEntitiesResponse200]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    claim_status: OperatorListEntitiesClaimStatus | Unset = UNSET,
    verification_method: OperatorListEntitiesVerificationMethod | Unset = UNSET,
    domain: str | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> Response[Error | OperatorListEntitiesResponse200]:
    """Story 48.4 — operator entity-admin: list/search claimed brands. Filters claim_status,
    verification_method, and a case-insensitive domain substring (filters AND together); limit/offset
    pagination (default limit 50, max 200). Operator-scoped: gated by the global ANSEO_OPERATOR_API_KEY
    (X-Anseo-API-Key); tenant project keys are rejected with 403. Reached server-to-server by the anseo-
    web BFF.

    Args:
        claim_status (OperatorListEntitiesClaimStatus | Unset):
        verification_method (OperatorListEntitiesVerificationMethod | Unset):
        domain (str | Unset):
        limit (int | Unset):  Default: 50.
        offset (int | Unset):  Default: 0.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorListEntitiesResponse200]
    """

    kwargs = _get_kwargs(
        claim_status=claim_status,
        verification_method=verification_method,
        domain=domain,
        limit=limit,
        offset=offset,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    claim_status: OperatorListEntitiesClaimStatus | Unset = UNSET,
    verification_method: OperatorListEntitiesVerificationMethod | Unset = UNSET,
    domain: str | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> Error | OperatorListEntitiesResponse200 | None:
    """Story 48.4 — operator entity-admin: list/search claimed brands. Filters claim_status,
    verification_method, and a case-insensitive domain substring (filters AND together); limit/offset
    pagination (default limit 50, max 200). Operator-scoped: gated by the global ANSEO_OPERATOR_API_KEY
    (X-Anseo-API-Key); tenant project keys are rejected with 403. Reached server-to-server by the anseo-
    web BFF.

    Args:
        claim_status (OperatorListEntitiesClaimStatus | Unset):
        verification_method (OperatorListEntitiesVerificationMethod | Unset):
        domain (str | Unset):
        limit (int | Unset):  Default: 50.
        offset (int | Unset):  Default: 0.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorListEntitiesResponse200
    """

    return sync_detailed(
        client=client,
        claim_status=claim_status,
        verification_method=verification_method,
        domain=domain,
        limit=limit,
        offset=offset,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    claim_status: OperatorListEntitiesClaimStatus | Unset = UNSET,
    verification_method: OperatorListEntitiesVerificationMethod | Unset = UNSET,
    domain: str | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> Response[Error | OperatorListEntitiesResponse200]:
    """Story 48.4 — operator entity-admin: list/search claimed brands. Filters claim_status,
    verification_method, and a case-insensitive domain substring (filters AND together); limit/offset
    pagination (default limit 50, max 200). Operator-scoped: gated by the global ANSEO_OPERATOR_API_KEY
    (X-Anseo-API-Key); tenant project keys are rejected with 403. Reached server-to-server by the anseo-
    web BFF.

    Args:
        claim_status (OperatorListEntitiesClaimStatus | Unset):
        verification_method (OperatorListEntitiesVerificationMethod | Unset):
        domain (str | Unset):
        limit (int | Unset):  Default: 50.
        offset (int | Unset):  Default: 0.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorListEntitiesResponse200]
    """

    kwargs = _get_kwargs(
        claim_status=claim_status,
        verification_method=verification_method,
        domain=domain,
        limit=limit,
        offset=offset,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    claim_status: OperatorListEntitiesClaimStatus | Unset = UNSET,
    verification_method: OperatorListEntitiesVerificationMethod | Unset = UNSET,
    domain: str | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> Error | OperatorListEntitiesResponse200 | None:
    """Story 48.4 — operator entity-admin: list/search claimed brands. Filters claim_status,
    verification_method, and a case-insensitive domain substring (filters AND together); limit/offset
    pagination (default limit 50, max 200). Operator-scoped: gated by the global ANSEO_OPERATOR_API_KEY
    (X-Anseo-API-Key); tenant project keys are rejected with 403. Reached server-to-server by the anseo-
    web BFF.

    Args:
        claim_status (OperatorListEntitiesClaimStatus | Unset):
        verification_method (OperatorListEntitiesVerificationMethod | Unset):
        domain (str | Unset):
        limit (int | Unset):  Default: 50.
        offset (int | Unset):  Default: 0.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorListEntitiesResponse200
    """

    return (
        await asyncio_detailed(
            client=client,
            claim_status=claim_status,
            verification_method=verification_method,
            domain=domain,
            limit=limit,
            offset=offset,
        )
    ).parsed
