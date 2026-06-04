"""Phase 2 Story 12.3 — auth helper that defaults to the OpenGEO header.

openapi-python-client@0.24 generates an ``AuthenticatedClient`` whose
defaults are ``prefix="Bearer"`` and ``auth_header_name="Authorization"``.
The OpenGEO API authenticates with ``X-OpenGEO-API-Key`` and no prefix
(architecture §5.1), so out-of-the-box consumers always have to override
both. This module exposes a factory that wires the correct defaults and
keeps every other ``AuthenticatedClient`` keyword pass-through.

Use this instead of constructing ``AuthenticatedClient`` directly:

    from opengeo.auth import OpenGeoClient
    client = OpenGeoClient(base_url="http://127.0.0.1:8080", api_key="ogeo_...")

This file is hand-written and lives alongside the auto-generated
package. It survives ``make sdks`` regeneration.
"""

from __future__ import annotations

from typing import Any

from .client import AuthenticatedClient

__all__ = ["OpenGeoClient"]


def OpenGeoClient(
    base_url: str,
    api_key: str,
    **kwargs: Any,
) -> AuthenticatedClient:
    """Construct an ``AuthenticatedClient`` pre-wired with the OpenGEO
    ``X-OpenGEO-API-Key`` header and no token prefix.

    Any other ``AuthenticatedClient`` keyword (``timeout``, ``verify_ssl``,
    ``httpx_args``, etc.) is forwarded verbatim.
    """
    kwargs.setdefault("prefix", "")
    kwargs.setdefault("auth_header_name", "X-OpenGEO-API-Key")
    return AuthenticatedClient(base_url=base_url, token=api_key, **kwargs)
