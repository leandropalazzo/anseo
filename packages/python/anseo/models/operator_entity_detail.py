from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.operator_verification_attempt import OperatorVerificationAttempt


T = TypeVar("T", bound="OperatorEntityDetail")


@_attrs_define
class OperatorEntityDetail:
    """Story 48.4 — entity detail: the OperatorEntity fields plus its verification_attempts (newest-first).

    Attributes:
        domain (str):
        verification_attempts (list[OperatorVerificationAttempt]):
        display_name (str | Unset):
        role (str | Unset):
        claim_status (str | Unset):
        verified_at (datetime.datetime | None | Unset):
        verification_method (None | str | Unset):
        grace_period_start (datetime.datetime | None | Unset):
        created_at (datetime.datetime | Unset):
        updated_at (datetime.datetime | Unset):
    """

    domain: str
    verification_attempts: list[OperatorVerificationAttempt]
    display_name: str | Unset = UNSET
    role: str | Unset = UNSET
    claim_status: str | Unset = UNSET
    verified_at: datetime.datetime | None | Unset = UNSET
    verification_method: None | str | Unset = UNSET
    grace_period_start: datetime.datetime | None | Unset = UNSET
    created_at: datetime.datetime | Unset = UNSET
    updated_at: datetime.datetime | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        domain = self.domain

        verification_attempts = []
        for verification_attempts_item_data in self.verification_attempts:
            verification_attempts_item = verification_attempts_item_data.to_dict()
            verification_attempts.append(verification_attempts_item)

        display_name = self.display_name

        role = self.role

        claim_status = self.claim_status

        verified_at: None | str | Unset
        if isinstance(self.verified_at, Unset):
            verified_at = UNSET
        elif isinstance(self.verified_at, datetime.datetime):
            verified_at = self.verified_at.isoformat()
        else:
            verified_at = self.verified_at

        verification_method: None | str | Unset
        if isinstance(self.verification_method, Unset):
            verification_method = UNSET
        else:
            verification_method = self.verification_method

        grace_period_start: None | str | Unset
        if isinstance(self.grace_period_start, Unset):
            grace_period_start = UNSET
        elif isinstance(self.grace_period_start, datetime.datetime):
            grace_period_start = self.grace_period_start.isoformat()
        else:
            grace_period_start = self.grace_period_start

        created_at: str | Unset = UNSET
        if not isinstance(self.created_at, Unset):
            created_at = self.created_at.isoformat()

        updated_at: str | Unset = UNSET
        if not isinstance(self.updated_at, Unset):
            updated_at = self.updated_at.isoformat()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "domain": domain,
                "verification_attempts": verification_attempts,
            }
        )
        if display_name is not UNSET:
            field_dict["display_name"] = display_name
        if role is not UNSET:
            field_dict["role"] = role
        if claim_status is not UNSET:
            field_dict["claim_status"] = claim_status
        if verified_at is not UNSET:
            field_dict["verified_at"] = verified_at
        if verification_method is not UNSET:
            field_dict["verification_method"] = verification_method
        if grace_period_start is not UNSET:
            field_dict["grace_period_start"] = grace_period_start
        if created_at is not UNSET:
            field_dict["created_at"] = created_at
        if updated_at is not UNSET:
            field_dict["updated_at"] = updated_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.operator_verification_attempt import OperatorVerificationAttempt

        d = dict(src_dict)
        domain = d.pop("domain")

        verification_attempts = []
        _verification_attempts = d.pop("verification_attempts")
        for verification_attempts_item_data in _verification_attempts:
            verification_attempts_item = OperatorVerificationAttempt.from_dict(
                verification_attempts_item_data
            )

            verification_attempts.append(verification_attempts_item)

        display_name = d.pop("display_name", UNSET)

        role = d.pop("role", UNSET)

        claim_status = d.pop("claim_status", UNSET)

        def _parse_verified_at(data: object) -> datetime.datetime | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                verified_at_type_0 = datetime.datetime.fromisoformat(data)

                return verified_at_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(datetime.datetime | None | Unset, data)

        verified_at = _parse_verified_at(d.pop("verified_at", UNSET))

        def _parse_verification_method(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        verification_method = _parse_verification_method(
            d.pop("verification_method", UNSET)
        )

        def _parse_grace_period_start(data: object) -> datetime.datetime | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                grace_period_start_type_0 = datetime.datetime.fromisoformat(data)

                return grace_period_start_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(datetime.datetime | None | Unset, data)

        grace_period_start = _parse_grace_period_start(
            d.pop("grace_period_start", UNSET)
        )

        _created_at = d.pop("created_at", UNSET)
        created_at: datetime.datetime | Unset
        if isinstance(_created_at, Unset):
            created_at = UNSET
        else:
            created_at = datetime.datetime.fromisoformat(_created_at)

        _updated_at = d.pop("updated_at", UNSET)
        updated_at: datetime.datetime | Unset
        if isinstance(_updated_at, Unset):
            updated_at = UNSET
        else:
            updated_at = datetime.datetime.fromisoformat(_updated_at)

        operator_entity_detail = cls(
            domain=domain,
            verification_attempts=verification_attempts,
            display_name=display_name,
            role=role,
            claim_status=claim_status,
            verified_at=verified_at,
            verification_method=verification_method,
            grace_period_start=grace_period_start,
            created_at=created_at,
            updated_at=updated_at,
        )

        operator_entity_detail.additional_properties = d
        return operator_entity_detail

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
