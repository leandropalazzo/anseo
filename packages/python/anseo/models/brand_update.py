from typing import TYPE_CHECKING, Any, TypeVar, Union, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.brand_competitor import BrandCompetitor


T = TypeVar("T", bound="BrandUpdate")


@_attrs_define
class BrandUpdate:
    """
    Attributes:
        name (str):
        variants (Union[Unset, list[str]]):
        competitors (Union[Unset, list['BrandCompetitor']]):
        site_url (Union[None, Unset, str]):
    """

    name: str
    variants: Union[Unset, list[str]] = UNSET
    competitors: Union[Unset, list["BrandCompetitor"]] = UNSET
    site_url: Union[None, Unset, str] = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        name = self.name

        variants: Union[Unset, list[str]] = UNSET
        if not isinstance(self.variants, Unset):
            variants = self.variants

        competitors: Union[Unset, list[dict[str, Any]]] = UNSET
        if not isinstance(self.competitors, Unset):
            competitors = []
            for competitors_item_data in self.competitors:
                competitors_item = competitors_item_data.to_dict()
                competitors.append(competitors_item)

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
        if competitors is not UNSET:
            field_dict["competitors"] = competitors
        if site_url is not UNSET:
            field_dict["site_url"] = site_url

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: dict[str, Any]) -> T:
        from ..models.brand_competitor import BrandCompetitor

        d = src_dict.copy()
        name = d.pop("name")

        variants = cast(list[str], d.pop("variants", UNSET))

        competitors = []
        _competitors = d.pop("competitors", UNSET)
        for competitors_item_data in _competitors or []:
            competitors_item = BrandCompetitor.from_dict(competitors_item_data)

            competitors.append(competitors_item)

        def _parse_site_url(data: object) -> Union[None, Unset, str]:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Union[None, Unset, str], data)

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
