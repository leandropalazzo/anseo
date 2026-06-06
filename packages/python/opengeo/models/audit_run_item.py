from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from dateutil.parser import isoparse
from typing import cast
import datetime






T = TypeVar("T", bound="AuditRunItem")



@_attrs_define
class AuditRunItem:
    """ 
        Attributes:
            id (str):
            target (str):
            overall_score (int):
            pages_crawled (int):
            created_at (datetime.datetime):
            gate_passed (bool | None | Unset):
     """

    id: str
    target: str
    overall_score: int
    pages_crawled: int
    created_at: datetime.datetime
    gate_passed: bool | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        id = self.id

        target = self.target

        overall_score = self.overall_score

        pages_crawled = self.pages_crawled

        created_at = self.created_at.isoformat()

        gate_passed: bool | None | Unset
        if isinstance(self.gate_passed, Unset):
            gate_passed = UNSET
        else:
            gate_passed = self.gate_passed


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "id": id,
            "target": target,
            "overall_score": overall_score,
            "pages_crawled": pages_crawled,
            "created_at": created_at,
        })
        if gate_passed is not UNSET:
            field_dict["gate_passed"] = gate_passed

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        id = d.pop("id")

        target = d.pop("target")

        overall_score = d.pop("overall_score")

        pages_crawled = d.pop("pages_crawled")

        created_at = isoparse(d.pop("created_at"))




        def _parse_gate_passed(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        gate_passed = _parse_gate_passed(d.pop("gate_passed", UNSET))


        audit_run_item = cls(
            id=id,
            target=target,
            overall_score=overall_score,
            pages_crawled=pages_crawled,
            created_at=created_at,
            gate_passed=gate_passed,
        )


        audit_run_item.additional_properties = d
        return audit_run_item

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
