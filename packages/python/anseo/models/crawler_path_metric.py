from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="CrawlerPathMetric")


@_attrs_define
class CrawlerPathMetric:
    """
    Attributes:
        error_hits (int):
        hits (int):
        path (str):
    """

    error_hits: int
    hits: int
    path: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        error_hits = self.error_hits

        hits = self.hits

        path = self.path

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "error_hits": error_hits,
                "hits": hits,
                "path": path,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        error_hits = d.pop("error_hits")

        hits = d.pop("hits")

        path = d.pop("path")

        crawler_path_metric = cls(
            error_hits=error_hits,
            hits=hits,
            path=path,
        )

        crawler_path_metric.additional_properties = d
        return crawler_path_metric

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
