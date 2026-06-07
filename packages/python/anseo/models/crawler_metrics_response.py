from __future__ import annotations

import datetime
from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.crawler_bot_metric import CrawlerBotMetric
    from ..models.crawler_path_metric import CrawlerPathMetric
    from ..models.crawler_trend_bucket import CrawlerTrendBucket


T = TypeVar("T", bound="CrawlerMetricsResponse")


@_attrs_define
class CrawlerMetricsResponse:
    """Roadmap Epic 31 crawler observability metrics. Headline payloads exclude unverified crawler hits unless
    `include_unverified=true` is requested.

        Attributes:
            bots (list[CrawlerBotMetric]):
            error_paths (list[CrawlerPathMetric]):
            include_unverified (bool):
            top_paths (list[CrawlerPathMetric]):
            trend (list[CrawlerTrendBucket]):
            window_end (datetime.datetime):
            window_start (datetime.datetime):
    """

    bots: list[CrawlerBotMetric]
    error_paths: list[CrawlerPathMetric]
    include_unverified: bool
    top_paths: list[CrawlerPathMetric]
    trend: list[CrawlerTrendBucket]
    window_end: datetime.datetime
    window_start: datetime.datetime
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        bots = []
        for bots_item_data in self.bots:
            bots_item = bots_item_data.to_dict()
            bots.append(bots_item)

        error_paths = []
        for error_paths_item_data in self.error_paths:
            error_paths_item = error_paths_item_data.to_dict()
            error_paths.append(error_paths_item)

        include_unverified = self.include_unverified

        top_paths = []
        for top_paths_item_data in self.top_paths:
            top_paths_item = top_paths_item_data.to_dict()
            top_paths.append(top_paths_item)

        trend = []
        for trend_item_data in self.trend:
            trend_item = trend_item_data.to_dict()
            trend.append(trend_item)

        window_end = self.window_end.isoformat()

        window_start = self.window_start.isoformat()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "bots": bots,
                "error_paths": error_paths,
                "include_unverified": include_unverified,
                "top_paths": top_paths,
                "trend": trend,
                "window_end": window_end,
                "window_start": window_start,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.crawler_bot_metric import CrawlerBotMetric
        from ..models.crawler_path_metric import CrawlerPathMetric
        from ..models.crawler_trend_bucket import CrawlerTrendBucket

        d = dict(src_dict)
        bots = []
        _bots = d.pop("bots")
        for bots_item_data in _bots:
            bots_item = CrawlerBotMetric.from_dict(bots_item_data)

            bots.append(bots_item)

        error_paths = []
        _error_paths = d.pop("error_paths")
        for error_paths_item_data in _error_paths:
            error_paths_item = CrawlerPathMetric.from_dict(error_paths_item_data)

            error_paths.append(error_paths_item)

        include_unverified = d.pop("include_unverified")

        top_paths = []
        _top_paths = d.pop("top_paths")
        for top_paths_item_data in _top_paths:
            top_paths_item = CrawlerPathMetric.from_dict(top_paths_item_data)

            top_paths.append(top_paths_item)

        trend = []
        _trend = d.pop("trend")
        for trend_item_data in _trend:
            trend_item = CrawlerTrendBucket.from_dict(trend_item_data)

            trend.append(trend_item)

        window_end = datetime.datetime.fromisoformat(d.pop("window_end"))

        window_start = datetime.datetime.fromisoformat(d.pop("window_start"))

        crawler_metrics_response = cls(
            bots=bots,
            error_paths=error_paths,
            include_unverified=include_unverified,
            top_paths=top_paths,
            trend=trend,
            window_end=window_end,
            window_start=window_start,
        )

        crawler_metrics_response.additional_properties = d
        return crawler_metrics_response

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
