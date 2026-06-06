from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AuditRequest")


@_attrs_define
class AuditRequest:
    """
    Attributes:
        target (str): URL, sitemap URL, file:// URL, or local HTML fixture path.
        max_pages (Union[None, Unset, int]):  Default: 25.
        timeout_ms (Union[None, Unset, int]):  Default: 10000.
        fail_on (Union[Unset, list[str]]): Rule ids or severities (low/medium/high).
    """

    target: str
    max_pages: Union[None, Unset, int] = 25
    timeout_ms: Union[None, Unset, int] = 10000
    fail_on: Union[Unset, list[str]] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        target = self.target

        max_pages: Union[None, Unset, int]
        if isinstance(self.max_pages, Unset):
            max_pages = UNSET
        else:
            max_pages = self.max_pages

        timeout_ms: Union[None, Unset, int]
        if isinstance(self.timeout_ms, Unset):
            timeout_ms = UNSET
        else:
            timeout_ms = self.timeout_ms

        fail_on: Union[Unset, list[str]] = UNSET
        if not isinstance(self.fail_on, Unset):
            fail_on = self.fail_on

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "target": target,
            }
        )
        if max_pages is not UNSET:
            field_dict["max_pages"] = max_pages
        if timeout_ms is not UNSET:
            field_dict["timeout_ms"] = timeout_ms
        if fail_on is not UNSET:
            field_dict["fail_on"] = fail_on

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        target = d.pop("target")

        def _parse_max_pages(data: object) -> Union[None, Unset, int]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, int], data)

        max_pages = _parse_max_pages(d.pop("max_pages", UNSET))

        def _parse_timeout_ms(data: object) -> Union[None, Unset, int]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, int], data)

        timeout_ms = _parse_timeout_ms(d.pop("timeout_ms", UNSET))

        fail_on = cast(list[str], d.pop("fail_on", UNSET))

        audit_request = cls(
            target=target,
            max_pages=max_pages,
            timeout_ms=timeout_ms,
            fail_on=fail_on,
        )

        audit_request.additional_properties = d
        return audit_request

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
