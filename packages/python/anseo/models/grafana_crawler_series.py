from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="GrafanaCrawlerSeries")


@_attrs_define
class GrafanaCrawlerSeries:
    """
    Attributes:
        datapoints (list[list[int]]):
        target (str):
    """

    datapoints: list[list[int]]
    target: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        datapoints = []
        for datapoints_item_data in self.datapoints:
            datapoints_item = datapoints_item_data

            datapoints.append(datapoints_item)

        target = self.target

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "datapoints": datapoints,
                "target": target,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        datapoints = []
        _datapoints = d.pop("datapoints")
        for datapoints_item_data in _datapoints:
            datapoints_item = cast(list[int], datapoints_item_data)

            datapoints.append(datapoints_item)

        target = d.pop("target")

        grafana_crawler_series = cls(
            datapoints=datapoints,
            target=target,
        )

        grafana_crawler_series.additional_properties = d
        return grafana_crawler_series

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
