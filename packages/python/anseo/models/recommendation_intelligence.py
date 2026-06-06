from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.kind_adoption import KindAdoption


T = TypeVar("T", bound="RecommendationIntelligence")


@_attrs_define
class RecommendationIntelligence:
    """
    Attributes:
        by_kind (list[KindAdoption]):
    """

    by_kind: list[KindAdoption]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        by_kind = []
        for by_kind_item_data in self.by_kind:
            by_kind_item = by_kind_item_data.to_dict()
            by_kind.append(by_kind_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "by_kind": by_kind,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.kind_adoption import KindAdoption

        d = dict(src_dict)
        by_kind = []
        _by_kind = d.pop("by_kind")
        for by_kind_item_data in _by_kind:
            by_kind_item = KindAdoption.from_dict(by_kind_item_data)

            by_kind.append(by_kind_item)

        recommendation_intelligence = cls(
            by_kind=by_kind,
        )

        recommendation_intelligence.additional_properties = d
        return recommendation_intelligence

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
