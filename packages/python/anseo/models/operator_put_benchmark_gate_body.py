from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="OperatorPutBenchmarkGateBody")


@_attrs_define
class OperatorPutBenchmarkGateBody:
    """
    Attributes:
        density_floor (int):
        terms_finalized (bool):
        terms_version (str):
        operator (str | Unset):
    """

    density_floor: int
    terms_finalized: bool
    terms_version: str
    operator: str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        density_floor = self.density_floor

        terms_finalized = self.terms_finalized

        terms_version = self.terms_version

        operator = self.operator

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "density_floor": density_floor,
                "terms_finalized": terms_finalized,
                "terms_version": terms_version,
            }
        )
        if operator is not UNSET:
            field_dict["operator"] = operator

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        density_floor = d.pop("density_floor")

        terms_finalized = d.pop("terms_finalized")

        terms_version = d.pop("terms_version")

        operator = d.pop("operator", UNSET)

        operator_put_benchmark_gate_body = cls(
            density_floor=density_floor,
            terms_finalized=terms_finalized,
            terms_version=terms_version,
            operator=operator,
        )

        operator_put_benchmark_gate_body.additional_properties = d
        return operator_put_benchmark_gate_body

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
