from typing import Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CreateProjectRequest")


@_attrs_define
class CreateProjectRequest:
    """
    Attributes:
        name (str):
        variants (Union[Unset, list[str]]):
        site_url (Union[None, Unset, str]):
    """

    name: str
    variants: Union[Unset, list[str]] = UNSET
    site_url: Union[None, Unset, str] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        name = self.name

        variants: Union[Unset, list[str]] = UNSET
        if not isinstance(self.variants, Unset):
            variants = self.variants

        site_url: Union[None, Unset, str]
        if isinstance(self.site_url, Unset):
            site_url = UNSET
        else:
            site_url = self.site_url

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "name": name,
            }
        )
        if variants is not UNSET:
            field_dict["variants"] = variants
        if site_url is not UNSET:
            field_dict["site_url"] = site_url

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        d = src_dict.copy()
        name = d.pop("name")

        variants = cast(list[str], d.pop("variants", UNSET))

        def _parse_site_url(data: object) -> Union[None, Unset, str]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, str], data)

        site_url = _parse_site_url(d.pop("site_url", UNSET))

        create_project_request = cls(
            name=name,
            variants=variants,
            site_url=site_url,
        )

        create_project_request.additional_properties = d
        return create_project_request

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
