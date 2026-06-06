from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.comparison_cell import ComparisonCell


T = TypeVar("T", bound="ComparisonRow")


@_attrs_define
class ComparisonRow:
    """
    Attributes:
        cells (list['ComparisonCell']):
        prompt_id (str): ULID.
        prompt_name (str):
        provider (str):
    """

    cells: list["ComparisonCell"]
    prompt_id: str
    prompt_name: str
    provider: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        cells = []
        for cells_item_data in self.cells:
            cells_item = cells_item_data.to_dict()
            cells.append(cells_item)

        prompt_id = self.prompt_id

        prompt_name = self.prompt_name

        provider = self.provider

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "cells": cells,
                "prompt_id": prompt_id,
                "prompt_name": prompt_name,
                "provider": provider,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        from ..models.comparison_cell import ComparisonCell

        d = src_dict.copy()
        cells = []
        _cells = d.pop("cells")
        for cells_item_data in _cells:
            cells_item = ComparisonCell.from_dict(cells_item_data)

            cells.append(cells_item)

        prompt_id = d.pop("prompt_id")

        prompt_name = d.pop("prompt_name")

        provider = d.pop("provider")

        comparison_row = cls(
            cells=cells,
            prompt_id=prompt_id,
            prompt_name=prompt_name,
            provider=provider,
        )

        comparison_row.additional_properties = d
        return comparison_row

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
