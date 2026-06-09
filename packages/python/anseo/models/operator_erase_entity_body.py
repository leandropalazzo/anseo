from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="OperatorEraseEntityBody")


@_attrs_define
class OperatorEraseEntityBody:
    """
    Attributes:
        confirm_token (str | Unset):
        operator (str | Unset):
    """

    confirm_token: str | Unset = UNSET
    operator: str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        confirm_token = self.confirm_token

        operator = self.operator

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if confirm_token is not UNSET:
            field_dict["confirm_token"] = confirm_token
        if operator is not UNSET:
            field_dict["operator"] = operator

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        confirm_token = d.pop("confirm_token", UNSET)

        operator = d.pop("operator", UNSET)

        operator_erase_entity_body = cls(
            confirm_token=confirm_token,
            operator=operator,
        )

        operator_erase_entity_body.additional_properties = d
        return operator_erase_entity_body

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
