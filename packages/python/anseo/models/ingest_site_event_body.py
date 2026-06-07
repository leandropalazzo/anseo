from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.ingest_site_event_body_properties import IngestSiteEventBodyProperties


T = TypeVar("T", bound="IngestSiteEventBody")


@_attrs_define
class IngestSiteEventBody:
    """
    Attributes:
        event_type (str): Must be one of the 10-event taxonomy: page_view, leaderboard_view, brand_profile_view,
            contribute_start, contribute_step, contribute_complete, verify_start, verify_complete, verify_fail,
            badge_embed_view. Unknown values are silently dropped (204).
        session_id (UUID): Ephemeral per-visit UUID generated client-side; not linked to identity.
        path (str | Unset): Site-relative path.
        properties (IngestSiteEventBodyProperties | Unset): Event-specific properties per the taxonomy.
        referrer (str | Unset): Referrer domain only (never a full URL).
    """

    event_type: str
    session_id: UUID
    path: str | Unset = UNSET
    properties: IngestSiteEventBodyProperties | Unset = UNSET
    referrer: str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        event_type = self.event_type

        session_id = str(self.session_id)

        path = self.path

        properties: dict[str, Any] | Unset = UNSET
        if not isinstance(self.properties, Unset):
            properties = self.properties.to_dict()

        referrer = self.referrer

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "event_type": event_type,
                "session_id": session_id,
            }
        )
        if path is not UNSET:
            field_dict["path"] = path
        if properties is not UNSET:
            field_dict["properties"] = properties
        if referrer is not UNSET:
            field_dict["referrer"] = referrer

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.ingest_site_event_body_properties import (
            IngestSiteEventBodyProperties,
        )

        d = dict(src_dict)
        event_type = d.pop("event_type")

        session_id = UUID(d.pop("session_id"))

        path = d.pop("path", UNSET)

        _properties = d.pop("properties", UNSET)
        properties: IngestSiteEventBodyProperties | Unset
        if isinstance(_properties, Unset):
            properties = UNSET
        else:
            properties = IngestSiteEventBodyProperties.from_dict(_properties)

        referrer = d.pop("referrer", UNSET)

        ingest_site_event_body = cls(
            event_type=event_type,
            session_id=session_id,
            path=path,
            properties=properties,
            referrer=referrer,
        )

        ingest_site_event_body.additional_properties = d
        return ingest_site_event_body

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
