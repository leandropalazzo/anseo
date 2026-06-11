import datetime
from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.error import Error
from ...models.operator_consent_events_event import OperatorConsentEventsEvent
from ...models.operator_consent_events_response_200 import (
    OperatorConsentEventsResponse200,
)
from ...models.operator_consent_events_tier import OperatorConsentEventsTier
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    tier: OperatorConsentEventsTier | Unset = UNSET,
    project: str | Unset = UNSET,
    event: OperatorConsentEventsEvent | Unset = UNSET,
    from_: datetime.datetime | Unset = UNSET,
    to: datetime.datetime | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    json_tier: str | Unset = UNSET
    if not isinstance(tier, Unset):
        json_tier = tier.value

    params["tier"] = json_tier

    params["project"] = project

    json_event: str | Unset = UNSET
    if not isinstance(event, Unset):
        json_event = event.value

    params["event"] = json_event

    json_from_: str | Unset = UNSET
    if not isinstance(from_, Unset):
        json_from_ = from_.isoformat()
    params["from"] = json_from_

    json_to: str | Unset = UNSET
    if not isinstance(to, Unset):
        json_to = to.isoformat()
    params["to"] = json_to

    params["limit"] = limit

    params["offset"] = offset

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/v1/operator/consent/events",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Error | OperatorConsentEventsResponse200 | None:
    if response.status_code == 200:
        response_200 = OperatorConsentEventsResponse200.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = Error.from_dict(response.json())

        return response_400

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
) -> Response[Error | OperatorConsentEventsResponse200]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    tier: OperatorConsentEventsTier | Unset = UNSET,
    project: str | Unset = UNSET,
    event: OperatorConsentEventsEvent | Unset = UNSET,
    from_: datetime.datetime | Unset = UNSET,
    to: datetime.datetime | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> Response[Error | OperatorConsentEventsResponse200]:
    """Story 49.0 (D1) — opt-in/opt-out event stream projected from the OSS-owned benchmark_consent ledger
    (event + terms_version + timestamp). Same filters/pagination as consent/records. Read-only.
    Operator-scoped; tenant keys 403.

    Args:
        tier (OperatorConsentEventsTier | Unset):
        project (str | Unset):
        event (OperatorConsentEventsEvent | Unset):
        from_ (datetime.datetime | Unset):
        to (datetime.datetime | Unset):
        limit (int | Unset):  Default: 50.
        offset (int | Unset):  Default: 0.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorConsentEventsResponse200]
    """

    kwargs = _get_kwargs(
        tier=tier,
        project=project,
        event=event,
        from_=from_,
        to=to,
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
    tier: OperatorConsentEventsTier | Unset = UNSET,
    project: str | Unset = UNSET,
    event: OperatorConsentEventsEvent | Unset = UNSET,
    from_: datetime.datetime | Unset = UNSET,
    to: datetime.datetime | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> Error | OperatorConsentEventsResponse200 | None:
    """Story 49.0 (D1) — opt-in/opt-out event stream projected from the OSS-owned benchmark_consent ledger
    (event + terms_version + timestamp). Same filters/pagination as consent/records. Read-only.
    Operator-scoped; tenant keys 403.

    Args:
        tier (OperatorConsentEventsTier | Unset):
        project (str | Unset):
        event (OperatorConsentEventsEvent | Unset):
        from_ (datetime.datetime | Unset):
        to (datetime.datetime | Unset):
        limit (int | Unset):  Default: 50.
        offset (int | Unset):  Default: 0.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorConsentEventsResponse200
    """

    return sync_detailed(
        client=client,
        tier=tier,
        project=project,
        event=event,
        from_=from_,
        to=to,
        limit=limit,
        offset=offset,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    tier: OperatorConsentEventsTier | Unset = UNSET,
    project: str | Unset = UNSET,
    event: OperatorConsentEventsEvent | Unset = UNSET,
    from_: datetime.datetime | Unset = UNSET,
    to: datetime.datetime | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> Response[Error | OperatorConsentEventsResponse200]:
    """Story 49.0 (D1) — opt-in/opt-out event stream projected from the OSS-owned benchmark_consent ledger
    (event + terms_version + timestamp). Same filters/pagination as consent/records. Read-only.
    Operator-scoped; tenant keys 403.

    Args:
        tier (OperatorConsentEventsTier | Unset):
        project (str | Unset):
        event (OperatorConsentEventsEvent | Unset):
        from_ (datetime.datetime | Unset):
        to (datetime.datetime | Unset):
        limit (int | Unset):  Default: 50.
        offset (int | Unset):  Default: 0.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Error | OperatorConsentEventsResponse200]
    """

    kwargs = _get_kwargs(
        tier=tier,
        project=project,
        event=event,
        from_=from_,
        to=to,
        limit=limit,
        offset=offset,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    tier: OperatorConsentEventsTier | Unset = UNSET,
    project: str | Unset = UNSET,
    event: OperatorConsentEventsEvent | Unset = UNSET,
    from_: datetime.datetime | Unset = UNSET,
    to: datetime.datetime | Unset = UNSET,
    limit: int | Unset = 50,
    offset: int | Unset = 0,
) -> Error | OperatorConsentEventsResponse200 | None:
    """Story 49.0 (D1) — opt-in/opt-out event stream projected from the OSS-owned benchmark_consent ledger
    (event + terms_version + timestamp). Same filters/pagination as consent/records. Read-only.
    Operator-scoped; tenant keys 403.

    Args:
        tier (OperatorConsentEventsTier | Unset):
        project (str | Unset):
        event (OperatorConsentEventsEvent | Unset):
        from_ (datetime.datetime | Unset):
        to (datetime.datetime | Unset):
        limit (int | Unset):  Default: 50.
        offset (int | Unset):  Default: 0.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Error | OperatorConsentEventsResponse200
    """

    return (
        await asyncio_detailed(
            client=client,
            tier=tier,
            project=project,
            event=event,
            from_=from_,
            to=to,
            limit=limit,
            offset=offset,
        )
    ).parsed
