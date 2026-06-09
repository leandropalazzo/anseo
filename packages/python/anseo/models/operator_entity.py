from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.operator_entity_claim_status import OperatorEntityClaimStatus
from ..models.operator_entity_role import OperatorEntityRole
from ..models.operator_entity_verification_method import (
    OperatorEntityVerificationMethod,
)
from ..types import UNSET, Unset

T = TypeVar("T", bound="OperatorEntity")


@_attrs_define
class OperatorEntity:
    """Story 48.4 — an entity (claimed brand) as seen by the operator entity-admin surface.

    Attributes:
        domain (str):
        display_name (str):
        role (OperatorEntityRole):
        claim_status (OperatorEntityClaimStatus):
        created_at (datetime.datetime):
        updated_at (datetime.datetime):
        verified_at (datetime.datetime | None | Unset):
        verification_method (OperatorEntityVerificationMethod | Unset):
        grace_period_start (datetime.datetime | None | Unset):
    """

    domain: str
    display_name: str
    role: OperatorEntityRole
    claim_status: OperatorEntityClaimStatus
    created_at: datetime.datetime
    updated_at: datetime.datetime
    verified_at: datetime.datetime | None | Unset = UNSET
    verification_method: OperatorEntityVerificationMethod | Unset = UNSET
    grace_period_start: datetime.datetime | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        domain = self.domain

        display_name = self.display_name

        role = self.role.value

        claim_status = self.claim_status.value

        created_at = self.created_at.isoformat()

        updated_at = self.updated_at.isoformat()

        verified_at: None | str | Unset
        if isinstance(self.verified_at, Unset):
            verified_at = UNSET
        elif isinstance(self.verified_at, datetime.datetime):
            verified_at = self.verified_at.isoformat()
        else:
            verified_at = self.verified_at

        verification_method: str | Unset = UNSET
        if not isinstance(self.verification_method, Unset):
            verification_method = self.verification_method.value

        grace_period_start: None | str | Unset
        if isinstance(self.grace_period_start, Unset):
            grace_period_start = UNSET
        elif isinstance(self.grace_period_start, datetime.datetime):
            grace_period_start = self.grace_period_start.isoformat()
        else:
            grace_period_start = self.grace_period_start

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "domain": domain,
                "display_name": display_name,
                "role": role,
                "claim_status": claim_status,
                "created_at": created_at,
                "updated_at": updated_at,
            }
        )
        if verified_at is not UNSET:
            field_dict["verified_at"] = verified_at
        if verification_method is not UNSET:
            field_dict["verification_method"] = verification_method
        if grace_period_start is not UNSET:
            field_dict["grace_period_start"] = grace_period_start

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        domain = d.pop("domain")

        display_name = d.pop("display_name")

        role = OperatorEntityRole(d.pop("role"))

        claim_status = OperatorEntityClaimStatus(d.pop("claim_status"))

        created_at = datetime.datetime.fromisoformat(d.pop("created_at"))

        updated_at = datetime.datetime.fromisoformat(d.pop("updated_at"))

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

        _verification_method = d.pop("verification_method", UNSET)
        verification_method: OperatorEntityVerificationMethod | Unset
        if isinstance(_verification_method, Unset):
            verification_method = UNSET
        else:
            verification_method = OperatorEntityVerificationMethod(_verification_method)

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

        operator_entity = cls(
            domain=domain,
            display_name=display_name,
            role=role,
            claim_status=claim_status,
            created_at=created_at,
            updated_at=updated_at,
            verified_at=verified_at,
            verification_method=verification_method,
            grace_period_start=grace_period_start,
        )

        operator_entity.additional_properties = d
        return operator_entity

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
