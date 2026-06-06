from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.visibility_sentiment_point import VisibilitySentimentPoint





T = TypeVar("T", bound="VisibilitySentimentResponse")



@_attrs_define
class VisibilitySentimentResponse:
    """ 
        Attributes:
            window_days (int):
            points (list[VisibilitySentimentPoint]):
     """

    window_days: int
    points: list[VisibilitySentimentPoint]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.visibility_sentiment_point import VisibilitySentimentPoint
        window_days = self.window_days

        points = []
        for points_item_data in self.points:
            points_item = points_item_data.to_dict()
            points.append(points_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "window_days": window_days,
            "points": points,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.visibility_sentiment_point import VisibilitySentimentPoint
        d = dict(src_dict)
        window_days = d.pop("window_days")

        points = []
        _points = d.pop("points")
        for points_item_data in (_points):
            points_item = VisibilitySentimentPoint.from_dict(points_item_data)



            points.append(points_item)


        visibility_sentiment_response = cls(
            window_days=window_days,
            points=points,
        )


        visibility_sentiment_response.additional_properties = d
        return visibility_sentiment_response

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
