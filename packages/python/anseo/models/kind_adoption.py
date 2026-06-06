from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="KindAdoption")


@_attrs_define
class KindAdoption:
    """
    Attributes:
        kind (str):
        surfaced (int):
        acted (int):
        dismissed (int):
        adoption_rate (Union[None, Unset, float]):
    """

    kind: str
    surfaced: int
    acted: int
    dismissed: int
    adoption_rate: Union[None, Unset, float] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        kind = self.kind

        surfaced = self.surfaced

        acted = self.acted

        dismissed = self.dismissed

        adoption_rate: Union[None, Unset, float]
        if isinstance(self.adoption_rate, Unset):
            adoption_rate = UNSET
        else:
            adoption_rate = self.adoption_rate

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "kind": kind,
                "surfaced": surfaced,
                "acted": acted,
                "dismissed": dismissed,
            }
        )
        if adoption_rate is not UNSET:
            field_dict["adoption_rate"] = adoption_rate

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        kind = d.pop("kind")

        surfaced = d.pop("surfaced")

        acted = d.pop("acted")

        dismissed = d.pop("dismissed")

        def _parse_adoption_rate(data: object) -> Union[None, Unset, float]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, float], data)

        adoption_rate = _parse_adoption_rate(d.pop("adoption_rate", UNSET))

        kind_adoption = cls(
            kind=kind,
            surfaced=surfaced,
            acted=acted,
            dismissed=dismissed,
            adoption_rate=adoption_rate,
        )

        kind_adoption.additional_properties = d
        return kind_adoption

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
