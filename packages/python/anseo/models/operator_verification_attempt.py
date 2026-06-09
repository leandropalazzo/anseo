from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import Any, TypeVar, cast
from uuid import UUID

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="OperatorVerificationAttempt")


@_attrs_define
class OperatorVerificationAttempt:
    """Story 48.4 — one row of an entity's append-only verification attempt history.

    Attributes:
        id (UUID):
        method (str):
        state (str):
        expires_at (datetime.datetime):
        created_at (datetime.datetime):
        consumed_at (datetime.datetime | None | Unset):
    """

    id: UUID
    method: str
    state: str
    expires_at: datetime.datetime
    created_at: datetime.datetime
    consumed_at: datetime.datetime | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        id = str(self.id)

        method = self.method

        state = self.state

        expires_at = self.expires_at.isoformat()

        created_at = self.created_at.isoformat()

        consumed_at: None | str | Unset
        if isinstance(self.consumed_at, Unset):
            consumed_at = UNSET
        elif isinstance(self.consumed_at, datetime.datetime):
            consumed_at = self.consumed_at.isoformat()
        else:
            consumed_at = self.consumed_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "id": id,
                "method": method,
                "state": state,
                "expires_at": expires_at,
                "created_at": created_at,
            }
        )
        if consumed_at is not UNSET:
            field_dict["consumed_at"] = consumed_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        id = UUID(d.pop("id"))

        method = d.pop("method")

        state = d.pop("state")

        expires_at = datetime.datetime.fromisoformat(d.pop("expires_at"))

        created_at = datetime.datetime.fromisoformat(d.pop("created_at"))

        def _parse_consumed_at(data: object) -> datetime.datetime | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                consumed_at_type_0 = datetime.datetime.fromisoformat(data)

                return consumed_at_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(datetime.datetime | None | Unset, data)

        consumed_at = _parse_consumed_at(d.pop("consumed_at", UNSET))

        operator_verification_attempt = cls(
            id=id,
            method=method,
            state=state,
            expires_at=expires_at,
            created_at=created_at,
            consumed_at=consumed_at,
        )

        operator_verification_attempt.additional_properties = d
        return operator_verification_attempt

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
