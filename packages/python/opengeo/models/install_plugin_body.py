from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset






T = TypeVar("T", bound="InstallPluginBody")



@_attrs_define
class InstallPluginBody:
    """ 
        Attributes:
            id (str):
            acknowledge_unsigned (bool | Unset):
     """

    id: str
    acknowledge_unsigned: bool | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        id = self.id

        acknowledge_unsigned = self.acknowledge_unsigned


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "id": id,
        })
        if acknowledge_unsigned is not UNSET:
            field_dict["acknowledge_unsigned"] = acknowledge_unsigned

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        id = d.pop("id")

        acknowledge_unsigned = d.pop("acknowledge_unsigned", UNSET)

        install_plugin_body = cls(
            id=id,
            acknowledge_unsigned=acknowledge_unsigned,
        )


        install_plugin_body.additional_properties = d
        return install_plugin_body

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
