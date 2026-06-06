import datetime
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field
from dateutil.parser import isoparse

from ..models.crawl_refer_state import CrawlReferState

if TYPE_CHECKING:
    from ..models.crawl_refer_ratio import CrawlReferRatio


T = TypeVar("T", bound="CrawlReferReport")


@_attrs_define
class CrawlReferReport:
    """Roadmap Epic 33 crawl-to-refer ratio. When referral attribution is unavailable, state is crawls_only and ratio is
    null.

        Attributes:
            window_start (datetime.datetime):
            window_end (datetime.datetime):
            state (CrawlReferState):
            bots (list['CrawlReferRatio']):
    """

    window_start: datetime.datetime
    window_end: datetime.datetime
    state: CrawlReferState
    bots: list["CrawlReferRatio"]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        window_start = self.window_start.isoformat()

        window_end = self.window_end.isoformat()

        state = self.state.value

        bots = []
        for bots_item_data in self.bots:
            bots_item = bots_item_data.to_dict()
            bots.append(bots_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "window_start": window_start,
                "window_end": window_end,
                "state": state,
                "bots": bots,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        from ..models.crawl_refer_ratio import CrawlReferRatio

        d = src_dict.copy()
        window_start = isoparse(d.pop("window_start"))

        window_end = isoparse(d.pop("window_end"))

        state = CrawlReferState(d.pop("state"))

        bots = []
        _bots = d.pop("bots")
        for bots_item_data in _bots:
            bots_item = CrawlReferRatio.from_dict(bots_item_data)

            bots.append(bots_item)

        crawl_refer_report = cls(
            window_start=window_start,
            window_end=window_end,
            state=state,
            bots=bots,
        )

        crawl_refer_report.additional_properties = d
        return crawl_refer_report

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
