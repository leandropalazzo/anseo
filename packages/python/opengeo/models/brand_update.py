from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.brand_competitor import BrandCompetitor





T = TypeVar("T", bound="BrandUpdate")



@_attrs_define
class BrandUpdate:
    """ 
        Attributes:
            name (str):
            variants (list[str] | Unset):
            competitors (list[BrandCompetitor] | Unset):
            site_url (None | str | Unset):
     """

    name: str
    variants: list[str] | Unset = UNSET
    competitors: list[BrandCompetitor] | Unset = UNSET
    site_url: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.brand_competitor import BrandCompetitor
        name = self.name

        variants: list[str] | Unset = UNSET
        if not isinstance(self.variants, Unset):
            variants = self.variants



        competitors: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.competitors, Unset):
            competitors = []
            for competitors_item_data in self.competitors:
                competitors_item = competitors_item_data.to_dict()
                competitors.append(competitors_item)



        site_url: None | str | Unset
        if isinstance(self.site_url, Unset):
            site_url = UNSET
        else:
            site_url = self.site_url


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "name": name,
        })
        if variants is not UNSET:
            field_dict["variants"] = variants
        if competitors is not UNSET:
            field_dict["competitors"] = competitors
        if site_url is not UNSET:
            field_dict["site_url"] = site_url

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.brand_competitor import BrandCompetitor
        d = dict(src_dict)
        name = d.pop("name")

        variants = cast(list[str], d.pop("variants", UNSET))


        _competitors = d.pop("competitors", UNSET)
        competitors: list[BrandCompetitor] | Unset = UNSET
        if _competitors is not UNSET:
            competitors = []
            for competitors_item_data in _competitors:
                competitors_item = BrandCompetitor.from_dict(competitors_item_data)



                competitors.append(competitors_item)


        def _parse_site_url(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        site_url = _parse_site_url(d.pop("site_url", UNSET))


        brand_update = cls(
            name=name,
            variants=variants,
            competitors=competitors,
            site_url=site_url,
        )


        brand_update.additional_properties = d
        return brand_update

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
