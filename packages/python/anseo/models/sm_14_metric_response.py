from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="Sm14MetricResponse")


@_attrs_define
class Sm14MetricResponse:
    """Story 19.5 — SM-14 adoption metric. `rate` is null when the denominator is zero.

    Attributes:
        denominator (int):
        numerator (int):
        rate (Union[None, Unset, float]):
    """

    denominator: int
    numerator: int
    rate: Union[None, Unset, float] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        denominator = self.denominator

        numerator = self.numerator

        rate: Union[None, Unset, float]
        if isinstance(self.rate, Unset):
            rate = UNSET
        else:
            rate = self.rate

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "denominator": denominator,
                "numerator": numerator,
            }
        )
        if rate is not UNSET:
            field_dict["rate"] = rate

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        denominator = d.pop("denominator")

        numerator = d.pop("numerator")

        def _parse_rate(data: object) -> Union[None, Unset, float]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, float], data)

        rate = _parse_rate(d.pop("rate", UNSET))

        sm_14_metric_response = cls(
            denominator=denominator,
            numerator=numerator,
            rate=rate,
        )

        sm_14_metric_response.additional_properties = d
        return sm_14_metric_response

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
