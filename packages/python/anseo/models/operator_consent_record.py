from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.operator_consent_record_event import OperatorConsentRecordEvent
from ..models.operator_consent_record_tier import OperatorConsentRecordTier
from ..types import UNSET, Unset

T = TypeVar("T", bound="OperatorConsentRecord")


@_attrs_define
class OperatorConsentRecord:
    """Story 49.0 — one row of the OSS-owned benchmark_consent ledger as seen by the Plane-1 operator read.

    Attributes:
        created_at (datetime.datetime):
        event (OperatorConsentRecordEvent):
        id (UUID):
        project_id (str):
        terms_version (str):
        tier (OperatorConsentRecordTier):
        actor (None | str | Unset):
        note (None | str | Unset):
    """

    created_at: datetime.datetime
    event: OperatorConsentRecordEvent
    id: UUID
    project_id: str
    terms_version: str
    tier: OperatorConsentRecordTier
    actor: None | str | Unset = UNSET
    note: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at.isoformat()

        event = self.event.value

        id = str(self.id)

        project_id = self.project_id

        terms_version = self.terms_version

        tier = self.tier.value

        actor: None | str | Unset
        if isinstance(self.actor, Unset):
            actor = UNSET
        else:
            actor = self.actor

        note: None | str | Unset
        if isinstance(self.note, Unset):
            note = UNSET
        else:
            note = self.note

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
        if actor is not UNSET:
            field_dict["actor"] = actor
        if note is not UNSET:
            field_dict["note"] = note

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = datetime.datetime.fromisoformat(d.pop("created_at"))

        event = OperatorConsentRecordEvent(d.pop("event"))

        id = UUID(d.pop("id"))

        project_id = d.pop("project_id")

        terms_version = d.pop("terms_version")

        tier = OperatorConsentRecordTier(d.pop("tier"))

        def _parse_actor(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        actor = _parse_actor(d.pop("actor", UNSET))

        def _parse_note(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        note = _parse_note(d.pop("note", UNSET))

        operator_consent_record = cls(
            created_at=created_at,
            event=event,
            id=id,
            project_id=project_id,
            terms_version=terms_version,
            tier=tier,
            actor=actor,
            note=note,
        )

        operator_consent_record.additional_properties = d
        return operator_consent_record

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
