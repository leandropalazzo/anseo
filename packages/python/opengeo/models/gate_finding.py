from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.gate_finding_severity import GateFindingSeverity






T = TypeVar("T", bound="GateFinding")



@_attrs_define
class GateFinding:
    """ 
        Attributes:
            page_url (str):
            rule_id (str):
            severity (GateFindingSeverity):
            message (str):
     """

    page_url: str
    rule_id: str
    severity: GateFindingSeverity
    message: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        page_url = self.page_url

        rule_id = self.rule_id

        severity = self.severity.value

        message = self.message


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "page_url": page_url,
            "rule_id": rule_id,
            "severity": severity,
            "message": message,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        page_url = d.pop("page_url")

        rule_id = d.pop("rule_id")

        severity = GateFindingSeverity(d.pop("severity"))




        message = d.pop("message")

        gate_finding = cls(
            page_url=page_url,
            rule_id=rule_id,
            severity=severity,
            message=message,
        )


        gate_finding.additional_properties = d
        return gate_finding

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
