from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.crawl_refer_state import CrawlReferState
from ..types import UNSET, Unset

T = TypeVar("T", bound="CrawlReferRatio")


@_attrs_define
class CrawlReferRatio:
    """
    Attributes:
        bot_id (str):
        verified_crawl_hits (int):
        attributed_referrals (int):
        state (CrawlReferState):
        ratio (float | None | Unset):
    """

    bot_id: str
    verified_crawl_hits: int
    attributed_referrals: int
    state: CrawlReferState
    ratio: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        bot_id = self.bot_id

        verified_crawl_hits = self.verified_crawl_hits

        attributed_referrals = self.attributed_referrals

        state = self.state.value

        ratio: float | None | Unset
        if isinstance(self.ratio, Unset):
            ratio = UNSET
        else:
            ratio = self.ratio

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "bot_id": bot_id,
                "verified_crawl_hits": verified_crawl_hits,
                "attributed_referrals": attributed_referrals,
                "state": state,
            }
        )
        if ratio is not UNSET:
            field_dict["ratio"] = ratio

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        bot_id = d.pop("bot_id")

        verified_crawl_hits = d.pop("verified_crawl_hits")

        attributed_referrals = d.pop("attributed_referrals")

        state = CrawlReferState(d.pop("state"))

        def _parse_ratio(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        ratio = _parse_ratio(d.pop("ratio", UNSET))

        crawl_refer_ratio = cls(
            bot_id=bot_id,
            verified_crawl_hits=verified_crawl_hits,
            attributed_referrals=attributed_referrals,
            state=state,
            ratio=ratio,
        )

        crawl_refer_ratio.additional_properties = d
        return crawl_refer_ratio

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
