from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.comparisons_response_window import ComparisonsResponseWindow
from typing import cast

if TYPE_CHECKING:
  from ..models.comparison_row import ComparisonRow





T = TypeVar("T", bound="ComparisonsResponse")



@_attrs_define
class ComparisonsResponse:
    """ Story 0.8 `GET /v1/comparisons` matrix payload — mirrors the MCP CompareBrandsOutput shape (architecture-phase3-mcp-
    server.md §3.3). Determinism contract: rows ordered (prompt_name ASC, provider ASC); cells ordered [brand,
    ...competitors_in_caller_order]; absent subjects carry ranking:null (NOT omitted).

        Attributes:
            brand (str):
            competitors (list[str]):
            rows (list[ComparisonRow]):
            trace_id (str):
            window (ComparisonsResponseWindow):
     """

    brand: str
    competitors: list[str]
    rows: list[ComparisonRow]
    trace_id: str
    window: ComparisonsResponseWindow
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.comparison_row import ComparisonRow
        brand = self.brand

        competitors = self.competitors



        rows = []
        for rows_item_data in self.rows:
            rows_item = rows_item_data.to_dict()
            rows.append(rows_item)



        trace_id = self.trace_id

        window = self.window.value


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "brand": brand,
            "competitors": competitors,
            "rows": rows,
            "trace_id": trace_id,
            "window": window,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.comparison_row import ComparisonRow
        d = dict(src_dict)
        brand = d.pop("brand")

        competitors = cast(list[str], d.pop("competitors"))


        rows = []
        _rows = d.pop("rows")
        for rows_item_data in (_rows):
            rows_item = ComparisonRow.from_dict(rows_item_data)



            rows.append(rows_item)


        trace_id = d.pop("trace_id")

        window = ComparisonsResponseWindow(d.pop("window"))




        comparisons_response = cls(
            brand=brand,
            competitors=competitors,
            rows=rows,
            trace_id=trace_id,
            window=window,
        )


        comparisons_response.additional_properties = d
        return comparisons_response

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
