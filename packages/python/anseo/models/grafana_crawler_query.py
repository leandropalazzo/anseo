from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="GrafanaCrawlerQuery")


@_attrs_define
class GrafanaCrawlerQuery:
    """
    Attributes:
        days (int | Unset):
        include_unverified (bool | Unset):  Default: False.
        target (str | Unset):
    """

    days: int | Unset = UNSET
    include_unverified: bool | Unset = False
    target: str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        days = self.days

        include_unverified = self.include_unverified

        target = self.target

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if days is not UNSET:
            field_dict["days"] = days
        if include_unverified is not UNSET:
            field_dict["include_unverified"] = include_unverified
        if target is not UNSET:
            field_dict["target"] = target

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        days = d.pop("days", UNSET)

        include_unverified = d.pop("include_unverified", UNSET)

        target = d.pop("target", UNSET)

        grafana_crawler_query = cls(
            days=days,
            include_unverified=include_unverified,
            target=target,
        )

        grafana_crawler_query.additional_properties = d
        return grafana_crawler_query

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
