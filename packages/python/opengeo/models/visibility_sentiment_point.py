from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="VisibilitySentimentPoint")



@_attrs_define
class VisibilitySentimentPoint:
    """ 
        Attributes:
            prompt (str):
            provider (str):
            entity (str):
            day (str):
            positive (int):
            neutral (int):
            negative (int):
            total (int):
            positive_share (float):
            neutral_share (float):
            negative_share (float):
            average_score (float):
     """

    prompt: str
    provider: str
    entity: str
    day: str
    positive: int
    neutral: int
    negative: int
    total: int
    positive_share: float
    neutral_share: float
    negative_share: float
    average_score: float
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        prompt = self.prompt

        provider = self.provider

        entity = self.entity

        day = self.day

        positive = self.positive

        neutral = self.neutral

        negative = self.negative

        total = self.total

        positive_share = self.positive_share

        neutral_share = self.neutral_share

        negative_share = self.negative_share

        average_score = self.average_score


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "prompt": prompt,
            "provider": provider,
            "entity": entity,
            "day": day,
            "positive": positive,
            "neutral": neutral,
            "negative": negative,
            "total": total,
            "positive_share": positive_share,
            "neutral_share": neutral_share,
            "negative_share": negative_share,
            "average_score": average_score,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        prompt = d.pop("prompt")

        provider = d.pop("provider")

        entity = d.pop("entity")

        day = d.pop("day")

        positive = d.pop("positive")

        neutral = d.pop("neutral")

        negative = d.pop("negative")

        total = d.pop("total")

        positive_share = d.pop("positive_share")

        neutral_share = d.pop("neutral_share")

        negative_share = d.pop("negative_share")

        average_score = d.pop("average_score")

        visibility_sentiment_point = cls(
            prompt=prompt,
            provider=provider,
            entity=entity,
            day=day,
            positive=positive,
            neutral=neutral,
            negative=negative,
            total=total,
            positive_share=positive_share,
            neutral_share=neutral_share,
            negative_share=negative_share,
            average_score=average_score,
        )


        visibility_sentiment_point.additional_properties = d
        return visibility_sentiment_point

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
