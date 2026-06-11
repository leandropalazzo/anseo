from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import Any, TypeVar
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.operator_consent_event_event import OperatorConsentEventEvent
from ..models.operator_consent_event_tier import OperatorConsentEventTier

T = TypeVar("T", bound="OperatorConsentEvent")


@_attrs_define
class OperatorConsentEvent:
    """Story 49.0 — an opt-in/opt-out event projected from benchmark_consent (event + terms_version + timestamp).

    Attributes:
        created_at (datetime.datetime):
        event (OperatorConsentEventEvent):
        id (UUID):
        project_id (str):
        terms_version (str):
        tier (OperatorConsentEventTier):
    """

    created_at: datetime.datetime
    event: OperatorConsentEventEvent
    id: UUID
    project_id: str
    terms_version: str
    tier: OperatorConsentEventTier
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at.isoformat()

        event = self.event.value

        id = str(self.id)

        project_id = self.project_id

        terms_version = self.terms_version

        tier = self.tier.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "created_at": created_at,
                "event": event,
                "id": id,
                "project_id": project_id,
                "terms_version": terms_version,
                "tier": tier,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = datetime.datetime.fromisoformat(d.pop("created_at"))

        event = OperatorConsentEventEvent(d.pop("event"))

        id = UUID(d.pop("id"))

        project_id = d.pop("project_id")

        terms_version = d.pop("terms_version")

        tier = OperatorConsentEventTier(d.pop("tier"))

        operator_consent_event = cls(
            created_at=created_at,
            event=event,
            id=id,
            project_id=project_id,
            terms_version=terms_version,
            tier=tier,
        )

        operator_consent_event.additional_properties = d
        return operator_consent_event

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
