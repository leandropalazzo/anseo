from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.audit_report import AuditReport
from ...models.audit_request import AuditRequest
from ...models.error import Error
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    body: AuditRequest,
    x_anseo_project: str | Unset = UNSET,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}
    if not isinstance(x_anseo_project, Unset):
        headers["X-Anseo-Project"] = x_anseo_project

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/v1/audit",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> AuditReport | Error | None:
    if response.status_code == 200:
        response_200 = AuditReport.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = Error.from_dict(response.json())

        return response_400

    if response.status_code == 401:
        response_401 = Error.from_dict(response.json())

        return response_401

    if response.status_code == 502:
        response_502 = Error.from_dict(response.json())

        return response_502

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[AuditReport | Error]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: AuditRequest,
    x_anseo_project: str | Unset = UNSET,
) -> Response[AuditReport | Error]:
    """Roadmap Epic 32 — crawl a URL/sitemap and score citation-readiness against in-tree heuristics
    (Identity, Extractability, Corroboration). Optional CI-gate thresholds.

    Args:
        x_anseo_project (str | Unset):
        body (AuditRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AuditReport | Error]
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
    client: AuthenticatedClient | Client,
    body: AuditRequest,
    x_anseo_project: str | Unset = UNSET,
) -> AuditReport | Error | None:
    """Roadmap Epic 32 — crawl a URL/sitemap and score citation-readiness against in-tree heuristics
    (Identity, Extractability, Corroboration). Optional CI-gate thresholds.

    Args:
        x_anseo_project (str | Unset):
        body (AuditRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AuditReport | Error
    """

    return sync_detailed(
        client=client,
        body=body,
        x_anseo_project=x_anseo_project,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: AuditRequest,
    x_anseo_project: str | Unset = UNSET,
) -> Response[AuditReport | Error]:
    """Roadmap Epic 32 — crawl a URL/sitemap and score citation-readiness against in-tree heuristics
    (Identity, Extractability, Corroboration). Optional CI-gate thresholds.

    Args:
        x_anseo_project (str | Unset):
        body (AuditRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[AuditReport | Error]
    """

    kwargs = _get_kwargs(
        body=body,
        x_anseo_project=x_anseo_project,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: AuditRequest,
    x_anseo_project: str | Unset = UNSET,
) -> AuditReport | Error | None:
    """Roadmap Epic 32 — crawl a URL/sitemap and score citation-readiness against in-tree heuristics
    (Identity, Extractability, Corroboration). Optional CI-gate thresholds.

    Args:
        x_anseo_project (str | Unset):
        body (AuditRequest):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        AuditReport | Error
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
            x_anseo_project=x_anseo_project,
        )
    ).parsed
