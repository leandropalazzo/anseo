from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.recommendation import Recommendation
  from ..models.transition_recommendation_response_warnings_item import TransitionRecommendationResponseWarningsItem





T = TypeVar("T", bound="TransitionRecommendationResponse")



@_attrs_define
class TransitionRecommendationResponse:
    """ 
        Attributes:
            recommendation (Recommendation): Story 19.6 — a stored GEO Recommendation (architecture-phase3-geo-
                recommendations.md §8 wire shape) plus its DB lifecycle `state`.
            warnings (list[TransitionRecommendationResponseWarningsItem]):
     """

    recommendation: Recommendation
    warnings: list[TransitionRecommendationResponseWarningsItem]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.recommendation import Recommendation
        from ..models.transition_recommendation_response_warnings_item import TransitionRecommendationResponseWarningsItem
        recommendation = self.recommendation.to_dict()

        warnings = []
        for warnings_item_data in self.warnings:
            warnings_item = warnings_item_data.to_dict()
            warnings.append(warnings_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "recommendation": recommendation,
            "warnings": warnings,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.recommendation import Recommendation
        from ..models.transition_recommendation_response_warnings_item import TransitionRecommendationResponseWarningsItem
        d = dict(src_dict)
        recommendation = Recommendation.from_dict(d.pop("recommendation"))




        warnings = []
        _warnings = d.pop("warnings")
        for warnings_item_data in (_warnings):
            warnings_item = TransitionRecommendationResponseWarningsItem.from_dict(warnings_item_data)



            warnings.append(warnings_item)


        transition_recommendation_response = cls(
            recommendation=recommendation,
            warnings=warnings,
        )


        transition_recommendation_response.additional_properties = d
        return transition_recommendation_response

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
