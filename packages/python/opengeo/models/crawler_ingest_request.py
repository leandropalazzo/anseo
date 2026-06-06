from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.crawler_ingest_request_format import CrawlerIngestRequestFormat
from ..models.crawler_ingest_request_privacy_mode import CrawlerIngestRequestPrivacyMode
from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="CrawlerIngestRequest")



@_attrs_define
class CrawlerIngestRequest:
    """ 
        Attributes:
            lines (list[str]): Raw access-log lines (nginx/Apache), one hit per line.
            format_ (CrawlerIngestRequestFormat | Unset):  Default: CrawlerIngestRequestFormat.COMBINED.
            privacy_mode (CrawlerIngestRequestPrivacyMode | Unset):  Default: CrawlerIngestRequestPrivacyMode.HASHED.
     """

    lines: list[str]
    format_: CrawlerIngestRequestFormat | Unset = CrawlerIngestRequestFormat.COMBINED
    privacy_mode: CrawlerIngestRequestPrivacyMode | Unset = CrawlerIngestRequestPrivacyMode.HASHED
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        lines = self.lines



        format_: str | Unset = UNSET
        if not isinstance(self.format_, Unset):
            format_ = self.format_.value


        privacy_mode: str | Unset = UNSET
        if not isinstance(self.privacy_mode, Unset):
            privacy_mode = self.privacy_mode.value



        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "lines": lines,
        })
        if format_ is not UNSET:
            field_dict["format"] = format_
        if privacy_mode is not UNSET:
            field_dict["privacy_mode"] = privacy_mode

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        lines = cast(list[str], d.pop("lines"))


        _format_ = d.pop("format", UNSET)
        format_: CrawlerIngestRequestFormat | Unset
        if isinstance(_format_,  Unset):
            format_ = UNSET
        else:
            format_ = CrawlerIngestRequestFormat(_format_)




        _privacy_mode = d.pop("privacy_mode", UNSET)
        privacy_mode: CrawlerIngestRequestPrivacyMode | Unset
        if isinstance(_privacy_mode,  Unset):
            privacy_mode = UNSET
        else:
            privacy_mode = CrawlerIngestRequestPrivacyMode(_privacy_mode)




        crawler_ingest_request = cls(
            lines=lines,
            format_=format_,
            privacy_mode=privacy_mode,
        )


        crawler_ingest_request.additional_properties = d
        return crawler_ingest_request

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
