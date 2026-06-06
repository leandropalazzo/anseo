from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.gate_finding import GateFinding





T = TypeVar("T", bound="GateSummary")



@_attrs_define
class GateSummary:
    """ 
        Attributes:
            passed (bool):
            fail_on (list[str]):
            failed_findings (list[GateFinding]):
     """

    passed: bool
    fail_on: list[str]
    failed_findings: list[GateFinding]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.gate_finding import GateFinding
        passed = self.passed

        fail_on = self.fail_on



        failed_findings = []
        for failed_findings_item_data in self.failed_findings:
            failed_findings_item = failed_findings_item_data.to_dict()
            failed_findings.append(failed_findings_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "passed": passed,
            "fail_on": fail_on,
            "failed_findings": failed_findings,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.gate_finding import GateFinding
        d = dict(src_dict)
        passed = d.pop("passed")

        fail_on = cast(list[str], d.pop("fail_on"))


        failed_findings = []
        _failed_findings = d.pop("failed_findings")
        for failed_findings_item_data in (_failed_findings):
            failed_findings_item = GateFinding.from_dict(failed_findings_item_data)



            failed_findings.append(failed_findings_item)


        gate_summary = cls(
            passed=passed,
            fail_on=fail_on,
            failed_findings=failed_findings,
        )


        gate_summary.additional_properties = d
        return gate_summary

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
