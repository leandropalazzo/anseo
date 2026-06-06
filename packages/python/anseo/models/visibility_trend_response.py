from typing import TYPE_CHECKING, Any, TypeVar, Union

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.visibility_trend_response_points_item import (
        VisibilityTrendResponsePointsItem,
    )


T = TypeVar("T", bound="VisibilityTrendResponse")


@_attrs_define
class VisibilityTrendResponse:
    """
    Attributes:
        points (Union[Unset, list['VisibilityTrendResponsePointsItem']]):
    """

    points: Union[Unset, list["VisibilityTrendResponsePointsItem"]] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        points: Union[Unset, list[dict[str, Any]]] = UNSET
        if not isinstance(self.points, Unset):
            points = []
            for points_item_data in self.points:
                points_item = points_item_data.to_dict()
                points.append(points_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if points is not UNSET:
            field_dict["points"] = points

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        from ..models.visibility_trend_response_points_item import (
            VisibilityTrendResponsePointsItem,
        )

        d = src_dict.copy()
        points = []
        _points = d.pop("points", UNSET)
        for points_item_data in _points or []:
            points_item = VisibilityTrendResponsePointsItem.from_dict(points_item_data)

            points.append(points_item)

        visibility_trend_response = cls(
            points=points,
        )

        visibility_trend_response.additional_properties = d
        return visibility_trend_response

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
