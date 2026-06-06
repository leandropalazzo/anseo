from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ComparisonCell")


@_attrs_define
class ComparisonCell:
    """
    Attributes:
        mention_count (int):
        subject (str):
        ranking (Union[None, Unset, int]):
    """

    mention_count: int
    subject: str
    ranking: Union[None, Unset, int] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        mention_count = self.mention_count

        subject = self.subject

        ranking: Union[None, Unset, int]
        if isinstance(self.ranking, Unset):
            ranking = UNSET
        else:
            ranking = self.ranking

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "mention_count": mention_count,
                "subject": subject,
            }
        )
        if ranking is not UNSET:
            field_dict["ranking"] = ranking

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        mention_count = d.pop("mention_count")

        subject = d.pop("subject")

        def _parse_ranking(data: object) -> Union[None, Unset, int]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, int], data)

        ranking = _parse_ranking(d.pop("ranking", UNSET))

        comparison_cell = cls(
            mention_count=mention_count,
            subject=subject,
            ranking=ranking,
        )

        comparison_cell.additional_properties = d
        return comparison_cell

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
